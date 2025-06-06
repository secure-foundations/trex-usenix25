#! /bin/bash

# This script runs a command on a remote server, copying an input file to the
# server and copying an output file back.
#
# It intelligently replaces instances of the input and output file names in the
# command with the actual paths on the server.
#
# It is the user's responsibility to make sure that the command can actually
# successfully run on the server.

function usage {
    echo "Usage: $0 [-f] [-t <remote_tempdir>] -i <input_file> -o <output_file> -n <server_name> -c <command>"
    echo "  -f: overwrite the output file if it exists"
    echo "  -i: input file, copied over to the server"
    echo "  -o: output file, copied back from the server"
    echo "  -n: server ssh identifier/name"
    echo "  -c: command to run on the server (any instances of the input file and output file will be replaced with the actual paths)"
    echo "  -t: assume a remote temporary directory on the server (default: picks a random one at /tmp/remoterunner_XXXXXXXXXXXX)"
    echo "  -h: help"
    exit 1
}

while getopts ":i:o:n:c:t:fh" opt; do
    case $opt in
    i)
        input_file="$OPTARG"
        ;;
    o)
        output_file="$OPTARG"
        ;;
    f)
        force=1
        ;;
    n)
        server_name="$OPTARG"
        ;;
    c)
        command="$OPTARG"
        ;;
    t)
        remote_tempdir="$OPTARG"
        ;;
    h)
        usage
        ;;
    :)
        echo "Option -$OPTARG requires an argument." >&2
        usage
        ;;
    \?)
        echo "Invalid option -$OPTARG" >&2
        usage
        ;;
    esac
done

if [ -z "$input_file" ]; then
    echo -e "Input file (-i) is required.\n"
    usage
fi
if [ -z "$output_file" ]; then
    echo -e "Output file (-o) is required.\n"
    usage
fi
if [ -z "$server_name" ]; then
    echo -e "Server name (-n) is required.\n"
    usage
fi
if [ -z "$command" ]; then
    echo -e "Command (-c) is required.\n"
    usage
fi

# Check if the input file exists
if [ ! -f "$input_file" ]; then
    echo "Input file $input_file does not exist."
    exit 1
fi

# Check if the output file exists
if [ -f "$output_file" ] && [ -z "$force" ]; then
    echo "Output file $output_file already exists. Please remove it before running the script, or pass '-f' to overwrite."
    exit 1
fi

# Set up SSH options and command
# Use ControlMaster to speed up SSH connections
SSH_OPTIONS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ControlPersist=1m -o ControlMaster=auto -o ControlPath=/tmp/remote_runner_$RANDOM"
echo "[INFO] Using SSH options: $SSH_OPTIONS"
SSH_CMD="ssh -Tn $SSH_OPTIONS $server_name"

if [ -n "$remote_tempdir" ]; then
    echo "[INFO] Using remote temporary directory: $remote_tempdir"
    SERVER_TEMPDIR="$remote_tempdir"
else
    echo "[INFO] No remote temporary directory (-t) specified, creating a random one..."
    echo "[INFO] Creating temporary directory on server $server_name..."
    SERVER_TEMPDIR=$($SSH_CMD mktemp -d /tmp/remoterunner_XXXXXXXXXXXX) || {
        echo "Failed to create temporary directory on server $server_name."
        exit 1
    }
fi

echo "[INFO] Temporary directory created on server at $SERVER_TEMPDIR"
scp $SSH_OPTIONS "$input_file" "$server_name:$SERVER_TEMPDIR/$(basename "$input_file")" || {
    echo "Failed to copy input file to server $server_name."
    exit 1
}

echo "[INFO] Replacing input and output file names in command..."
# First, we escape any special characters in the input and output file names
input_file_escaped=$(printf '%s\n' "$input_file" | sed 's/[\[$]/\\&/g')
output_file_escaped=$(printf '%s\n' "$output_file" | sed 's/[\[$]/\\&/g')
REPLACED_CMD=$(echo "$command" | sed "s|$input_file_escaped|$SERVER_TEMPDIR/$(basename "$input_file_escaped")|g; s|$output_file_escaped|$SERVER_TEMPDIR/$(basename "$output_file_escaped")|g")
echo "       Command to run on server: $REPLACED_CMD"

RNG="$RANDOM"
STDOUT_STDERR_FILE="$SERVER_TEMPDIR/stdout_stderr_$RNG.log"
PID_FILE="$SERVER_TEMPDIR/$RNG.pid"
EXIT_CODE_FILE="$SERVER_TEMPDIR/$RNG.exit_code"

echo "[INFO] Spinning up command on server $server_name..."
$SSH_CMD "nohup bash -c '($REPLACED_CMD); echo \$? > $EXIT_CODE_FILE' > '$STDOUT_STDERR_FILE' 2>&1 & echo \$! > $PID_FILE" || {
    echo "Command failed on server $server_name."
    exit 1
}

kill_remote() {
    echo "[INFO] Killing remote runner if it exists..."
    $SSH_CMD "if [ -f $PID_FILE ]; then kill \$(cat $PID_FILE) 2>/dev/null; rm -f $PID_FILE; fi"
}
trap kill_remote EXIT

echo "[INFO] Starting wait loop..."
while true; do
    if $SSH_CMD "if [ -f $PID_FILE ] && kill -0 \$(cat $PID_FILE) 2>/dev/null; then echo 'Running'; else echo 'Not running'; fi" | grep -q 'Not running'; then
        break
    fi
    # We should dump the output from the server to stdout, and truncate the
    # server file so that we get the latest output on next iteration
    $SSH_CMD "[ -s $STDOUT_STDERR_FILE ] && cat $STDOUT_STDERR_FILE; : >$STDOUT_STDERR_FILE" || {
        echo "Failed to read output from server $server_name."
        exit 1
    }
    sleep 1
done

# Since it is no longer running, we can remove the PID file
$SSH_CMD "if [ -f $PID_FILE ]; then rm -f $PID_FILE; fi"
echo "[INFO] Command completed on server $server_name."

echo "[INFO] Checking exit code..."
exit_code=$($SSH_CMD "if [ -f $EXIT_CODE_FILE ]; then cat $EXIT_CODE_FILE; else echo '1'; fi")
if [ "$exit_code" -ne 0 ]; then
    echo "Command failed with exit code $exit_code."
    exit 1
fi

echo "[INFO] Copying output file back to local machine..."
scp $SSH_OPTIONS "$server_name:$SERVER_TEMPDIR/$(basename "$output_file")" "$output_file" || {
    echo "Failed to copy output file from server $server_name."
    exit 1
}

if [ -n "$remote_tempdir" ]; then
    echo "[INFO] Not cleaning up temporary directory on server $server_name as it was specified by the user."
else
    echo "[INFO] Cleaning up temporary directory on server $server_name..."
    $SSH_CMD rm -rf "$SERVER_TEMPDIR" || {
        echo "Failed to clean up temporary directory on server $server_name."
        exit 1
    }
fi

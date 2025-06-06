//Recover types from Decompiler View, taking variables as input
//@author Jay Bosamiya
//@category Exporter
//@keybinding
//@menupath
//@toolbar

import java.io.File;
import java.util.List;
import java.util.Scanner;

import ghidra.app.plugin.core.analysis.AutoAnalysisManager;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSpace;
import ghidra.program.model.data.DataType;
import ghidra.program.model.data.Undefined;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.listing.LocalVariableImpl;
import ghidra.program.model.listing.ParameterImpl;
import ghidra.program.model.listing.Variable;
import ghidra.program.model.symbol.SourceType;
import ghidra.util.exception.DuplicateNameException;

public class TypesRecovererWithVariableInputs extends TypesDump {
	@Override
	protected void run() throws Exception {
		this.input_variables();
		this.rerun_analysis();
		if (this.getScriptArgs().length == 1) {
			this.dump_types("types-recovered-with-var-inputs", true, null);
		} else {
			String arg = this.getScriptArgs()[1];
			String prefix = "singlefunc=";
			if (arg.startsWith(prefix)) {
				String single_func = arg.replaceFirst(prefix, "");
				this.dump_types("types-recovered-with-var-inputs", true, single_func);
			} else {
				throw new Exception("Unexpected argument, expected singlefunc=...");
			}
		}
	}

	void rerun_analysis() throws Exception {
		this.resetAllAnalysisOptions(currentProgram);
		this.enableHeadlessAnalysis(true);
		AutoAnalysisManager.getAnalysisManager(currentProgram).startAnalysis(monitor);
		this.analyzeAll(currentProgram);
	}

	void input_variables() throws Exception {
		String input_file_name = this.getScriptArgs()[0];
		if (!input_file_name.endsWith(".vars")) {
			throw new Exception("Expected .vars file path as argument");
		}

		File input_file = new File(input_file_name);
		if (!input_file.exists()) {
			throw new Exception("Expected " + input_file_name + " but not found.");
		}
		Scanner reader = new Scanner(input_file);
		String line;

		// PROGRAM section
		line = reader.nextLine().stripTrailing();
		if(!line.equals("PROGRAM")) {
			reader.close();
			throw new Exception("Expected PROGRAM, got " + line);
		}
		reader.nextLine(); // name
		reader.nextLine(); // stack pointer
		reader.nextLine(); // blank line

		// VARIABLES section
		line = reader.nextLine().stripTrailing();
		if(!line.equals("VARIABLES")) {
			reader.close();
			throw new Exception("Expected VARIABLES, got " + line);
		}
		Listing listing = currentProgram.getListing();
		line = null;
		if(reader.hasNextLine()) {
			line = reader.nextLine().stripTrailing();
		}
		while(line != null) {
			if (line.isBlank()) {
				line = null;
				if(reader.hasNextLine()) {
					line = reader.nextLine().stripTrailing();
				}
				continue;
			}
			if (!(line.startsWith("\t") && !line.startsWith("\t\t"))) {
				reader.close();
				throw new Exception("Parse error. Got line " + line);
			}
			String varname = line.strip().split("@")[0];
			String funcname = line.strip().split("@")[1];
			int funcaddr = Integer.parseInt(line.strip().split("@")[2], 16);
			Function f = null;
			for (Function it : listing.getFunctions(true)) {
				if (it.getEntryPoint().getOffset() == funcaddr) {
					f = it;
				}
			}
			if (f != null) {
				f.setName(funcname, SourceType.IMPORTED);
			}
			line = null;
			if(reader.hasNextLine()) {
				line = reader.nextLine().stripTrailing();
			}
			while (line != null && line.startsWith("\t\t")) {
				String[] varnode_loc = line.replace('(', ' ').replace(')', ' ').strip().split(",");
				String loc_aspc = varnode_loc[0].strip();
				String loc_offset = varnode_loc[1].strip();
				int loc_size = Integer.parseInt(varnode_loc[2].strip());

				AddressSpace aspc = currentProgram.getCompilerSpec().getAddressSpace(loc_aspc);
				Address addr = aspc.getAddress(loc_aspc + ":" + loc_offset);

				Variable var;
				if (loc_aspc.equals("register")) {
					var = new ParameterImpl(varname, Undefined.getUndefinedDataType(loc_size), addr, currentProgram);
				} else if (loc_aspc.equals("stack")) {
					var = new LocalVariableImpl(varname, 0, Undefined.getUndefinedDataType(loc_size), addr, currentProgram);
				} else {
					reader.close();
					throw new Exception("Unknown address space" + line);
				}
				try {
					if (f != null) {
						f.addLocalVariable(var, SourceType.IMPORTED);
					}
				} catch (DuplicateNameException e) {
					// Do nothing if it already exists
				}
				line = null;
				if(reader.hasNextLine()) {
					line = reader.nextLine().stripTrailing();
				}
			}
		}

		reader.close();
	}
}

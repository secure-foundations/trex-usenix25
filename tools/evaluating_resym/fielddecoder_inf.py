import json
import torch
from transformers import AutoTokenizer, AutoModelForCausalLM
import argparse
import time
from huggingface_hub import login
import os 
import signal

class TimeoutException(Exception):
    pass

def handler(_signum, _frame):
    raise TimeoutException()

signal.signal(signal.SIGALRM, handler)

hf_key = os.environ['HF_TOKEN']
login(token = hf_key)

def inference(test_fpath, out_fpath, model_path):
    print('==========start loading model==========')
    
    tokenizer = AutoTokenizer.from_pretrained('bigcode/starcoderbase-3b', use_auth_token=hf_key)
    model = AutoModelForCausalLM.from_pretrained(
        model_path, use_auth_token=hf_key,
        torch_dtype=torch.bfloat16, device_map='auto'
    )

    wp = open(out_fpath, 'w')

    print('==========start inference==========')
    with open(test_fpath, 'r') as fp:
        for i, line in enumerate(fp.readlines()):
            try:
                signal.alarm(5) # Set a timeout per-function
                line = json.loads(line)
                first_token = line['input'].split(':')[-1].split(',')[0].split('?')[0]
                prompt = line['input'] + first_token + ':'

                start_time = time.time()

                input_ids = tokenizer.encode(prompt, return_tensors='pt').cuda()[:, : 8192 - 1024]
                output = model.generate(
                    input_ids=input_ids, max_new_tokens=1024, num_beams=4, num_return_sequences=1, do_sample=False,
                    early_stopping=False, pad_token_id=tokenizer.eos_token_id, eos_token_id=tokenizer.eos_token_id
                )[0]
                output = tokenizer.decode(output[input_ids.size(1): ], skip_special_tokens=True, clean_up_tokenization_spaces=True)
                output = first_token + ':' + output

                time_used = time.time() - start_time
                save_data = line
                save_data['predict'] = output
                save_data['time'] = time_used
                wp.write(json.dumps(save_data) + '\n')
            except TimeoutException:
                print(f'Line {i} timeout. Skipping.')
                if isinstance(line, str):
                    line = json.loads(line)
                save_data = line
                save_data['timeout'] = True
                wp.write(json.dumps(save_data) + '\n')
                continue
            except Exception as e:
                raise e
            finally:
                # Cancel the alarm (next iteration will set it again)
                signal.alarm(0)


if __name__=='__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('test_fpath')
    parser.add_argument('out_fpath')
    parser.add_argument('model_path')
    args = parser.parse_args()

    inference(args.test_fpath, args.out_fpath, args.model_path)

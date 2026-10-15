
### Contract probe: `Qwen3.5-4B-Q4_K_M` at `http://127.0.0.1:8030/v1`

| Probe | Verdict | HTTP | s | Notes |
|---|---|---|---|---|
| GET /models | ✓ | 200 | 0.01 | ids=['default'] |
| GET /effective-config | ✓ | 200 | 0.00 | ok |
| chat plain | ✓ | 200 | 21.75 | fields=['choices', 'created', 'id', 'model', 'object', 'usage'] usage={'prompt_tokens': 55, 'completion_tokens': 39, 'total_tokens': 94} finish=stop |
| tools auto -> tool_call | ✓ | 200 | 108.74 | finish=tool_calls id=call_0 type=function name=get_weather args={'city': 'Paris', 'unit': 'celsius'} args_is_str=True |
| tool result -> final answer | ✗ | 422 | 0.00 | finish=None tool_calls=False content="" |
| tool_choice=required | ✗ | 200 | 130.82 | finish=stop got_call=False want_call=True err= |
| tool_choice=none | ✓ | 200 | 116.88 | finish=stop got_call=False want_call=False err= |
| 10 tools -> picks convert_currency | ✓ | 200 | 268.88 | finish=tool_calls name=convert_currency args={"amount":250,"from":"EUR","to":"USD"} prompt_tokens=926 |
| stream + tools | ✓ | 200 | 97.19 | ctype=text/event-stream events=3 tool_call_deltas=1 finish=['tool_calls'] done=True |
| stream plain | ✓ | 200 | 18.44 | events=44 done=True usage_in_stream=True text="Here is the count from one to ten in words:\n\n1. One\n2. Two\n3. Three\n4. Fou |
| extra OpenAI fields tolerated | ✓ | 200 | 4.97 | err=ok |
| max_completion_tokens honoured | ✗ | 200 | 62.08 | completion_tokens=256 finish=length |
| error envelope {error:{message,type}} | ✗ | 422 | 0.00 | {"error": "Invalid request body. Check that the JSON structure matches the expected schema."} |
| foreign model id | ℹ | 200 | 4.29 | served_as=gpt-4o-mini status=200 |
| no Authorization header | ℹ | 200 | 2.29 | accepted (no auth at the server; the front door must enforce it) |

Full record: `results/probe-4b.json`

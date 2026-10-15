
### Contract probe: `Qwen3.5-0.8B-Q4_K_M` at `http://127.0.0.1:8029/v1`

| Probe | Verdict | HTTP | s | Notes |
|---|---|---|---|---|
| GET /models | ✓ | 200 | 0.01 | ids=['default'] |
| GET /effective-config | ✓ | 200 | 0.00 | ok |
| chat plain | ✓ | 200 | 4.92 | fields=['choices', 'created', 'id', 'model', 'object', 'usage'] usage={'prompt_tokens': 55, 'completion_tokens': 31, 'total_tokens': 86} finish=stop |
| tools auto -> tool_call | ✓ | 200 | 26.92 | finish=tool_calls id=call_0 type=function name=get_weather args={'city': 'Paris', 'unit': 'celsius'} args_is_str=True |
| tool result -> final answer | ✗ | 422 | 0.00 | finish=None tool_calls=False content="" |
| tool_choice=required | ✗ | 200 | 22.14 | finish=stop got_call=False want_call=True err= |
| tool_choice=none | ✓ | 200 | 23.97 | finish=stop got_call=False want_call=False err= |
| 10 tools -> picks convert_currency | ✓ | 200 | 57.76 | finish=tool_calls name=convert_currency args={"amount":250,"from":"eur","to":"usd"} prompt_tokens=926 |
| stream + tools | ✓ | 200 | 23.95 | ctype=text/event-stream events=3 tool_call_deltas=1 finish=['tool_calls'] done=True |
| stream plain | ✓ | 200 | 4.08 | events=44 done=True usage_in_stream=True text="Here is the count from one to ten in words:\n\n1. One\n2. Two\n3. Three\n4. Fou |
| extra OpenAI fields tolerated | ✓ | 200 | 1.21 | err=ok |
| max_completion_tokens honoured | ✗ | 200 | 15.25 | completion_tokens=244 finish=stop |
| error envelope {error:{message,type}} | ✗ | 422 | 0.00 | {"error": "Invalid request body. Check that the JSON structure matches the expected schema."} |
| foreign model id | ℹ | 200 | 1.15 | served_as=gpt-4o-mini status=200 |
| no Authorization header | ℹ | 200 | 0.62 | accepted (no auth at the server; the front door must enforce it) |

Full record: `results/probe-0.8b.json`

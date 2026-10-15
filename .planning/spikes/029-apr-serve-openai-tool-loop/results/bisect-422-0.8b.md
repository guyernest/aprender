
### 422 bisect: `Qwen3.5-0.8B-Q4_K_M` at `http://127.0.0.1:8029/v1`

| Variant | HTTP | s | Notes |
|---|---|---|---|
| A assistant{tool_calls, no content} + tool{tool_call_id} | 422 | 0.0 | {"error": "Invalid request body. Check that the JSON structure matches the expected schema."} |
| B assistant{content:null, tool_calls} + tool | 422 | 0.0 | {"error": "Invalid request body. Check that the JSON structure matches the expected schema."} |
| C assistant{content:'', tool_calls} + tool | 200 | 48.6 | finish=stop tool_calls=False content="The weather in Paris is currently **light rain** with a temperature of **17\u00b0C**." |
| D assistant{content:text} + tool{tool_call_id} | 200 | 49.5 | finish=stop tool_calls=False content="It's currently light rain in Paris. The temperature is 17\u00b0C." |
| E assistant{content:text} + tool{no tool_call_id} | 200 | 3.4 | finish=stop tool_calls=False content="It's currently light rain in Paris. The temperature is 17\u00b0C." |
| F assistant{content:'', tool_calls} only | 200 | 49.1 | finish=stop tool_calls=False content="I don't have access to real-time weather data, so I can't tell you the current weather in Paris. To get the l |
| G assistant{content:text} + user{result} | 200 | 47.1 | finish=stop tool_calls=False content="The weather in Paris is currently light rain." |

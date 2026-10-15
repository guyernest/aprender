## Cross-config summary (medians; M4 Pro, alone on the host)

| config | prefill tok/s | decode tok/s | RSS MB | tools2 turn s (390 tok) | tools10 turn s (926 tok) | long2k s | long4k s |
|---|---|---|---|---|---|---|---|
| 0.8B-t14 | 16.5 | 17.9 | 2402 | 25.8 | 57.4 | 118.1 | 263.1 |
| 0.8B-t6 | 12.3 | 13.1 | 2394 | 35.1 | 77.8 | 154.0 | 327.5 |
| 2B-t14 | 8.9 | 9.2 | 4788 | 49.3 | 108.2 | 214.2 | 444.8 |
| 2B-t6 | 5.9 | 6.1 | 4722 | 76.5 | 165.8 | 325.0 |  |
| 4B-t14 | 4.2 | 4.4 | 5568 | 102.6 | 225.9 | 468.6 |  |
| 4B-t6 | 3.0 | 3.0 | 5620 | 156.0 | 337.6 |  |  |

## Prefill rate by prompt length (flat = token-at-a-time)

| config | short | tools2 | tools10 | long2k | long4k |
|---|---|---|---|---|---|
| 0.8B-t14 | 17.9 tok/s @ 73 | 16.4 tok/s @ 421 | 16.4 tok/s @ 937 | 16.1 tok/s @ 1891 | 14.4 tok/s @ 3770 |
| 0.8B-t6 | 13.3 tok/s @ 73 | 12.0 tok/s @ 421 | 12.1 tok/s @ 937 | 12.4 tok/s @ 1890 | 11.6 tok/s @ 3770 |
| 2B-t14 | 9.7 tok/s @ 73 | 8.6 tok/s @ 421 | 8.6 tok/s @ 937 | 8.9 tok/s @ 1891 | 8.5 tok/s @ 3770 |
| 2B-t6 | 6.0 tok/s @ 72 | 5.5 tok/s @ 420 | 5.7 tok/s @ 937 | 5.9 tok/s @ 1891 |  |
| 4B-t14 | 4.3 tok/s @ 73 | 4.1 tok/s @ 421 | 4.2 tok/s @ 937 | 4.1 tok/s @ 1891 |  |
| 4B-t6 | 3.0 tok/s @ 73 | 2.7 tok/s @ 421 | 2.8 tok/s @ 937 |  |  |

## Turn-2 session reuse (TTFT s)

| config | turn1 TTFT (prompt tok) | turn2 extend TTFT (prompt tok) | turn2 after a foreign request TTFT | reuse factor |
|---|---|---|---|---|
| 0.8B-t14 | 26.0 (421) | 2.8 (460) | 29.3 | 10.6x |
| 0.8B-t6 | 35.6 (421) | 3.6 (460) | 35.2 | 9.7x |
| 2B-t14 | 49.2 (421) | 5.2 (460) | 49.4 | 9.4x |
| 2B-t6 | 76.2 (421) | 7.6 (460) | 75.8 | 10.0x |
| 4B-t14 | 104.4 (421) | 10.8 (460) | 104.9 | 9.8x |
| 4B-t6 | 153.9 (421) | 15.5 (460) | 153.6 | 9.9x |

## Concurrency (short requests, 64 output tokens)

| config | 1 request total s | 2 at once: wall s / per-request s | 4 at once: wall s / per-request s | verdict |
|---|---|---|---|---|
| 0.8B-t14 | 8.1 | 15.4 / [15.4, 7.6] | 31.0 / [15.5, 31.0, 23.2, 7.7] | serialises |
| 0.8B-t6 | 11.2 | 20.7 / [10.3, 20.7] | 31.2 / [10.3, 25.9, 20.6, 31.2] | serialises |
| 2B-t14 | 17.0 | 29.4 / [29.4, 14.6] | 57.9 / [14.6, 29.1, 57.9, 43.5] | serialises |
| 2B-t6 | 25.2 | 45.2 / [22.2, 45.2] | 77.6 / [55.6, 77.6, 22.1, 33.5] | serialises |
| 4B-t14 | 35.0 | 60.4 / [29.8, 60.4] | 102.2 / [29.0, 102.2, 72.9, 57.8] | serialises |
| 4B-t6 | 48.4 | 89.6 / [89.6, 44.8] | 158.4 / [45.5, 158.4, 113.5, 90.3] | serialises |

## Agent-turn model and Graviton2 projection (est., x3.2)

| config | 2-tool turn (390+38) M4 s | est. Graviton2 s | 10-tool turn (926+53) M4 s | est. Graviton2 s | 2k handbook (2000+64) M4 s | est. Graviton2 s |
|---|---|---|---|---|---|---|
| 0.8B-t14 | 26 | 82 | 59 | 189 | 125 | 399 |
| 0.8B-t6 | 35 | 111 | 79 | 254 | 167 | 536 |
| 2B-t14 | 48 | 153 | 110 | 351 | 232 | 741 |
| 2B-t6 | 72 | 231 | 166 | 530 | 349 | 1118 |
| 4B-t14 | 101 | 325 | 233 | 744 | 491 | 1570 |
| 4B-t6 | 143 | 457 | 326 | 1044 | 688 | 2202 |

## Tool choice correctness

- 0.8B-t14: tools2 → [['get_weather'], ['get_weather']], tools10 → [['convert_currency'], ['convert_currency']]
- 0.8B-t6: tools2 → [['get_weather'], ['get_weather']], tools10 → [['convert_currency'], ['convert_currency']]
- 2B-t14: tools2 → [['get_weather'], ['get_weather']], tools10 → [['convert_currency'], ['convert_currency']]
- 2B-t6: tools2 → [['get_weather'], ['get_weather']], tools10 → [['convert_currency'], ['convert_currency']]
- 4B-t14: tools2 → [['get_weather'], ['get_weather']], tools10 → [['convert_currency'], ['convert_currency']]
- 4B-t6: tools2 → [['get_weather'], ['get_weather']], tools10 → [['convert_currency']]

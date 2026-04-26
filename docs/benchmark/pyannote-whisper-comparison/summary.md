# pyannote whisper comparison

| model | success | mean | median | min | max | mean chars | JP ratio | space ratio |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| openai_gpt4o | 30/30 | 6.001s | 6.259s | 4.259s | 7.398s | 527.7 | 0.883 | 0.001 |
| pyannote_parakeet | 30/30 | 14.636s | 14.315s | 13.233s | 25.163s | 344.7 | 0.000 | 0.118 |
| pyannote_whisper | 30/30 | 16.145s | 14.302s | 13.647s | 26.606s | 950.6 | 0.913 | 0.345 |

## openai_gpt4o
- success: 30/30
- mean seconds: 6.001
- median seconds: 6.259
- min seconds: 4.259
- max seconds: 7.398
- mean text chars: 527.7
- mean Japanese-char ratio: 0.883
- mean space ratio: 0.001

  - capture-000001.wav: n=5, mean=6.471s, median=6.489s
  - capture-000002.wav: n=5, mean=6.615s, median=6.549s
  - capture-000003.wav: n=5, mean=6.384s, median=6.453s
  - capture-000004.wav: n=5, mean=4.939s, median=4.745s
  - capture-000005.wav: n=5, mean=5.381s, median=5.301s
  - capture-000006.wav: n=5, mean=6.218s, median=6.280s

## pyannote_parakeet
- success: 30/30
- mean seconds: 14.636
- median seconds: 14.315
- min seconds: 13.233
- max seconds: 25.163
- mean text chars: 344.7
- mean Japanese-char ratio: 0.000
- mean space ratio: 0.118

  - capture-000001.wav: n=5, mean=13.783s, median=13.556s
  - capture-000002.wav: n=5, mean=16.671s, median=14.861s
  - capture-000003.wav: n=5, mean=14.384s, median=14.356s
  - capture-000004.wav: n=5, mean=14.154s, median=14.023s
  - capture-000005.wav: n=5, mean=14.373s, median=14.371s
  - capture-000006.wav: n=5, mean=14.455s, median=14.447s

## pyannote_whisper
- success: 30/30
- mean seconds: 16.145
- median seconds: 14.302
- min seconds: 13.647
- max seconds: 26.606
- mean text chars: 950.6
- mean Japanese-char ratio: 0.913
- mean space ratio: 0.345

  - capture-000001.wav: n=5, mean=14.341s, median=14.394s
  - capture-000002.wav: n=5, mean=16.432s, median=14.049s
  - capture-000003.wav: n=5, mean=18.622s, median=14.548s
  - capture-000004.wav: n=5, mean=14.088s, median=13.806s
  - capture-000005.wav: n=5, mean=16.816s, median=14.779s
  - capture-000006.wav: n=5, mean=16.571s, median=14.001s

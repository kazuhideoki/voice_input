# Apple SpeechTranscriber と gpt-live-transcribe の日本語比較

## 結論

2026年8月18日時点では、日本語入力の既定providerをApple
`SpeechTranscriber`へ置き換える根拠は得られなかった。正解文つき合成音声3件では、現行
`gpt-live-transcribe`のCERが12.0%、Appleの精度優先設定が12.8%、Talkifyと同じ
reporting optionsを使うfast設定が15.2%だった。

Appleの精度優先設定は現行providerとほぼ同水準であり、ローカル完結、API費用不要、
ネットワーク不要という利点がある。一方、Talkifyの応答性を支える`fastResults`では、
言い淀みや技術語の崩れが増えた。Apple経路を導入する場合は、既定providerの置換ではなく、
オフラインまたはプライバシー優先用途の明示的な選択肢として再評価する。

## 調査範囲

同一の日本語音声に対して、次の3条件を比較した。

- Apple `SpeechTranscriber`の精度優先設定。確定結果のみを使用した。
- Apple `SpeechTranscriber`の`volatileResults`と`fastResults`。Talkifyの文字起こし設定と
  同じだが、Talkifyアプリ全体のマイク入力、事前ウォームアップ、文字挿入処理は再現していない。
- voice-inputの現行`gpt-live-transcribe`設定。日本語、low delay、24 kHz PCM、100 ms単位で、
  音声の実時間に合わせて逐次送信した。

正解文つき比較にはmacOSのKyoko音声で生成した3件、自然音声の観察には
`diarize-log-storage`のrun `20260818T140644_385+0900`から先頭45秒を使用した。
自然音声には人手で確定した正解文がないため、CERや勝敗数を算出していない。

対象外は次のとおり。

- Talkifyアプリのキー解放から文字表示までのend-to-end遅延
- Appleのマイク入力を使った停止から確定までの遅延
- 雑音、話者、発話速度を変えた網羅的な日本語品質評価
- 話者分離品質の比較

## 実行条件

| 項目 | 値 |
|---|---|
| 実行日 | 2026-08-18、Asia/Tokyo |
| OS | macOS 26.5.2、build 25F84、arm64 |
| voice-input | `464ed6c57f3ec112f5380d61811bcb97d111710e` |
| Talkify参照 | `af41d64f63ac2fc94861280fd5fdf2f1b1df75da` |
| GPT model | `gpt-live-transcribe` |
| GPT session | `languages=["ja"]`、`delay="low"`、turn detectionなし |
| Apple locale | `ja-JP` |

Apple側はTalkifyの`FileTranscriptionService`を基礎に、ファイルを
`SpeechAnalyzer.analyzeSequence`へ渡した。fast条件ではreporting optionsをTalkifyのライブ設定へ
合わせ、volatileな途中結果をCERへ混入させず確定結果だけを連結した。

GPT側は各サンプルで新しいWebSocketを作成した。ただし遅延評価では接続時間を含む全体時間ではなく、
voice-inputが録音中に接続と送信を進める構成に合わせ、`input_audio_buffer.commit`から
`conversation.item.input_audio_transcription.completed`までを測った。

詳細な環境、音声hash、設定は[manifest.json](manifest.json)、各出力は[results.json](results.json)に残す。

実装条件の根拠は、voice-inputの
[gpt-live-transcribe adapter](../../../src/infrastructure/external/gpt_live_transcribe_adapter.rs)と、
固定したTalkify commitの
[SpeechRecognitionService.swift](https://github.com/tornikegomareli/Talkify/blob/af41d64f63ac2fc94861280fd5fdf2f1b1df75da/Talkify/Dictation/SpeechRecognitionService.swift)
である。

## 正解文つき合成音声

### 音声

| ID | 長さ | 正解文 |
|---|---:|---|
| gold_1 | 8.238秒 | macOSを中心に、シェル設定、エディタ設定、各種CLIをまとめて管理するリポジトリです。 |
| gold_2 | 9.988秒 | Raspberry Pi 4では、USB全体の電流が1.2アンペアを超えると、緑キャプチャーが切断される可能性があります。 |
| gold_3 | 5.812秒 | えっと、昨日は朝まず面接を一人して、その方はお見送りにしました。 |

CERは正解文と出力をNFKCで正規化し、casefoldした後、句読点、記号、空白を除外して文字単位の
Levenshtein距離から算出した。3件合計の正解文は125文字である。

### 結果

| Provider | gold_1 | gold_2 | gold_3 | 合計CER |
|---|---:|---:|---:|---:|
| Apple・精度優先 | 12.2% | 20.0% | 0.0% | 12.8%（16/125） |
| Apple・fast | 12.2% | 20.0% | 10.3% | 15.2%（19/125） |
| gpt-live-transcribe | 14.6% | 16.4% | 0.0% | **12.0%（15/125）** |

主要な差は次のとおり。

- `各種CLI`はApple精度優先が`各シクリード`、Apple fastが`各シクリー用`、GPTが
  `各種クエリ`と認識した。3条件とも正解ではない。
- `Raspberry Pi 4`はApple精度優先が`ラスクベリー杯 4`、Apple fastが
  `ラスクベリー第 4`、GPTが`ラスプベリーPi 4`と認識した。
- `えっと`はApple精度優先とGPTが正しく認識したが、Apple fastは`ネット`と認識した。

この3件ではGPTが最小CERだったが、Apple精度優先との差は1文字である。小規模な単一話者の
合成音声だけから、一般的な日本語精度の優劣までは結論づけない。

## 遅延

| Provider | 測定対象 | 結果 |
|---|---|---:|
| Apple・精度優先 | ファイル全体の解析 | 平均0.381秒 |
| Apple・fast | ファイル全体の解析 | 平均0.396秒 |
| gpt-live-transcribe | commitからcompleted | 平均0.617秒、0.533〜0.702秒 |

Appleは5.8〜10.0秒のファイルを0.14〜0.86秒で解析した。GPTは録音中に音声を実時間送信した後、
停止相当のcommitから0.53〜0.70秒で確定した。測定区間が異なるため、この表からAppleが
一律に何秒速いとは判断しない。Talkifyが公開している英語のキー解放から表示までのベンチマークも、
今回の日本語ファイル試験とは別の測定である。

## diarize-log自然音声での観察

使用した45秒区間は、耐久テストとキャプチャーデバイスについて複数人が会話する音声である。
保存済み`gpt-transcribe`結果は話者分離済みsegments、Apple結果は単一の連続テキストであり、
出力構造も一致しない。保存済みGPT結果は人手正解ではなく、比較用の観察値としてのみ扱った。

確認できた差は次のとおり。

- 保存済みGPTは`SCMチーム`を認識したが、`緑キャプチャー`を`ミドルキャプチャー`とした。
- Apple精度優先は`SCMチーム`を`エムチーム`としたが、`緑キャプチャー`を認識した。
- Apple fastは`耐久テスト`を`耐久テース`、`パターン`を`パパー`とし、精度優先より崩れが増えた。
- 45秒のファイル処理時間はApple精度優先が1.087秒、Apple fastが1.227秒だった。

この自然音声は今回OpenAIへ新規送信していない。GPT側は`diarize-log-storage`にすでに保存されていた
`gpt-transcribe`結果であり、正解文でも現行`gpt-live-transcribe`の結果でもない。

## 判断と制約

確認済みの結果から、次の判断とする。

- 現行`gpt-live-transcribe`を日本語入力の既定providerとして維持する。
- Appleのfast設定を、速度だけを理由に既定providerへ採用しない。
- Apple経路は、ローカル完結、ネットワーク不要、API費用不要を必要とする場合に改めて検討する。
- Apple経路を実装判断する前に、実話者、技術語、言い淀みを含む正解文つき30〜50発話で再比較する。

残る制約は次のとおり。

- 合成音声は1話者3件のみで、Apple音声合成からApple音声認識への偏りがあり得る。
- 英字の技術語をmacOS音声合成がどう発音したかと、期待する表記を復元できたかが混在している。
- 各条件は1回のみで、モデル出力の試行間変動を評価していない。
- CERは`CLI`を`クエリ`とするような意味上大きい誤りと、表記揺れを同じ文字編集距離で扱う。
- 自然音声に人手正解がないため、自然会話での定量的な優劣は未確認である。

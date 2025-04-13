# Voice Input

A script that enables text input using OpenAI's Speech to Text API.
You can start and stop recording at any desired timing.

## Feature

- When the script is launched for the first time, it starts recording; the second time, it stops recording and sends a request to the Speech-to-Text API. Then it pastes the result to the clipboard.
- The cursor position or selected text is included in the request as context.

## Environment Variables

You can customize settings using environment variables:

- `OPENAI_API_KEY`: OpenAI API key (required)
- `OPENAI_TRANSCRIBE_MODEL`: Transcription model to use (default: `gpt-4o-mini-transcribe`)

Please refer to the `.env.example` file for configuration examples.

## How to Use

wip

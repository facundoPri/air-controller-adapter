# Local transcription

Recording uses your selected Mac or headset microphone. It does not assume the MR-01 has a microphone. Transcription is optional and requires an executable and model you have already installed. The app does not download models or send audio to a transcription service.

No transcription engine is bundled. Install and configure your preferred engine before enabling text insertion.

## Executable contract

`TranscriptionCommand` accepts this structure (adapt the example; no executable is included):

```json
{
  "program": "/absolute/path/to/my-transcriber",
  "args": ["--audio", "{audio}"],
  "insert_text": true
}
```

`program` identifies a local executable. Prefer an absolute path because an app opened from Finder may have a different `PATH` from Terminal. Each `args` entry is a separate argument; at least one must contain `{audio}`, which is replaced with the WAV path. Do not add shell quotes inside the JSON: spaces, `$`, semicolons, and other characters are passed literally. The app does not invoke a shell.

The executable must write **only the final UTF-8 transcription to stdout**, exit with code 0, and send progress messages to stderr. A wrapper can invoke your local engine, read its text output file, and print it. Check your engine version's options: not every engine prints clean text to stdout. A configured wrapper is trusted code; the app cannot prevent that program from making its own network requests.

Limits are 120 seconds and 64 KiB per output stream (stdout and stderr). Empty output, invalid UTF-8, process errors, and time or size overruns are rejected. Process diagnostics are not copied to the application log. On macOS and Unix, cancellation terminates the command's process group; on other platforms, it terminates the direct process.

`insert_text` is a preference consumed by the voice workflow. The transcription runner returns text; it does not type into another application by itself. Recording, transcription, and insertion should have visible states; an error message must never be inserted as dictated text.

## Validate before dictating

1. Test your executable manually with a sample recording and verify that stdout contains only the spoken text.
2. Configure the engine object, select an audio input, and assign the voice action to an observable remote button.
3. Test recording and transcription with a short phrase. The app needs Microphone permission; insertion may require Accessibility.
4. Focus a test document before starting dictation to test insertion. Avoid password fields or a terminal as your first destination.

Automated runner tests use local fixture executables. They check literal arguments, empty text, UTF-8, process errors, both stream limits, and timeout cancellation. They do not replace an end-to-end recording → real engine → target application test.

# J.A.R.V.I.S.

A voice-interactive Python assistant that listens to your commands and responds through speech.

## Features

- 🎤 Voice input using microphone
- 🗣️ Speech-to-text conversion
- 💬 Text-to-speech responses
- 🔄 Continuous listening mode
- 📝 Command logging

## Dependencies

- Python 3.x
- SpeechRecognition
- pyttsx3
- PyAudio


## Install dependencies
This only installs imported dependencies and accordingly update the requirements.txt
```bash
pip install pipreqs
pipreqs . --force
```

## Setup Instructions

1. Create a virtual environment and install dependencies:
```bash
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

2. Create a launchd plist file in `~/Library/LaunchAgents/com.user.jarvis.plist` with the following content:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.jarvis</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/deepak.terse/Code/jarvis/venv/bin/python</string>
        <string>/Users/deepak.terse/Code/jarvis/src/main.py</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>/Users/deepak.terse/Code/jarvis/logs/jarvis.err</string>
    <key>StandardOutPath</key>
    <string>/Users/deepak.terse/Code/jarvis/logs/jarvis.out</string>
    <key>WorkingDirectory</key>
    <string>/Users/deepak.terse/Code/jarvis</string>
</dict>
</plist>
```

3. Set the correct permissions for the plist file:
```bash
chmod 644 ~/Library/LaunchAgents/com.user.jarvis.plist
```

4. Load and start the service:
```bash
# Unload if previously loaded
launchctl unload ~/Library/LaunchAgents/com.user.jarvis.plist

# Load the service
launchctl load ~/Library/LaunchAgents/com.user.jarvis.plist

# Start the service
launchctl start com.user.jarvis
```

## Usage

Once JARVIS is running, it will:
1. Listen for voice input through your microphone
2. Convert your speech to text
3. Process your command
4. Respond through text-to-speech

To exit JARVIS, simply say "exit", "quit", or "stop".

## Monitoring

You can monitor the daemon's output through:
- Main log file: `/tmp/jarvis.log`
- Standard output: `/tmp/jarvis.out`
- Error log: `/tmp/jarvis.err`

To check the service status:
```bash
launchctl list | grep jarvis
```

## Troubleshooting

If JARVIS isn't working as expected:
1. Check the error logs at `/logs/jarvis.err`
2. Verify your microphone is properly connected and has necessary permissions
3. Ensure all dependencies are installed correctly
4. Check if the Python script path in the plist file matches your actual script location
5. Confirm the Python interpreter path is correct (verify with `which python3`)
6. Ensure you have write permissions to the `/tmp` directory

## Running the app

- Using `launchctl` (as mentioned above)
- Using `python3 src/main.py`


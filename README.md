# J.A.R.V.I.S.

A simple Python daemon that prints "Hello, World!" every minute.

## Setup Instructions

1. Create a launchd plist file in `~/Library/LaunchAgents/com.user.jarvis.plist` with the following content:

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
    <string>/tmp/jarvis.err</string>
    <key>StandardOutPath</key>
    <string>/tmp/jarvis.out</string>
    <key>WorkingDirectory</key>
    <string>/Users/deepak.terse/Code/jarvis</string>
</dict>
</plist>
```

2. Set the correct permissions for the plist file:
```bash
chmod 644 ~/Library/LaunchAgents/com.user.jarvis.plist
```

3. Load and start the service:
```bash
# Unload if previously loaded
launchctl unload ~/Library/LaunchAgents/com.user.jarvis.plist

# Load the service
launchctl load ~/Library/LaunchAgents/com.user.jarvis.plist

# Start the service
launchctl start com.user.jarvis
```

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

If the daemon isn't running as expected:
1. Check the error logs at `/tmp/jarvis.err`
2. Verify the Python script path in the plist file matches your actual script location
3. Confirm the Python interpreter path is correct (verify with `which python3`)
4. Ensure you have write permissions to the `/tmp` directory


## Running the app

- Using `launchctl` (as mentioned above)
- Using `python3 src/main.py`

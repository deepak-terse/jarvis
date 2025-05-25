# speaker.py
from actuators.tts import speak_text

def respond(text):
    speak_text(text)

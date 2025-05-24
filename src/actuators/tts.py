# text_to_speech.py
import pyttsx3

engine = pyttsx3.init()

def speak_text(text):
    print(f"💬 Speaking: {text}")
    engine.say(text)
    engine.runAndWait()

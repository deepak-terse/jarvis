# text_to_speech.py
import pyttsx3
import logging

logging.basicConfig(level=logging.INFO)
engine = pyttsx3.init()

def speak_text(text):
    logging.info(f"💬 Speaking: {text}")
    engine.say(text)
    engine.runAndWait()

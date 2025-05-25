# speech_to_text.py
import speech_recognition as sr
import logging

logging.basicConfig(level=logging.INFO)

def transcribe(audio):
    recognizer = sr.Recognizer()
    try:
        text = recognizer.recognize_google(audio)
        logging.info(f"📝 Recognized: {text}")
        return text
    except sr.UnknownValueError:
        return "Sorry, I didn't catch that."
    except sr.RequestError:
        return "Speech service unavailable."

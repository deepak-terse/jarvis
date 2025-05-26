# Using XTTS
from TTS.api import TTS
import os

# Initialize model (will auto-download if missing)
model = TTS(
    model_name="tts_models/multilingual/multi-dataset/xtts_v2",
    progress_bar=True,
    gpu=False  # Set to True if you have CUDA
)

def speak(text, speaker_wav="jarvis_sample.wav"):
    if not os.path.exists(speaker_wav):
        raise FileNotFoundError(f"Speaker file {speaker_wav} not found")
    
    output_path = "output.wav"
    model.tts_to_file(
        text=text,
        speaker_wav=speaker_wav,
        language="en",
        file_path=output_path
    )
    return output_path

# Usage
audio_file = speak("Hello, I am JARVIS")
print(f"Generated: {audio_file}")
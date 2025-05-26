# Using VCTK
from TTS.api import TTS
import sounddevice as sd
import scipy.io.wavfile
import tempfile
import os

# Load a pre-trained model (offline, realistic voice)
tts = TTS(model_name="tts_models/en/vctk/vits", progress_bar=False, gpu=False)

# Text to convert
text = "Hello, I am your virtual assistant. How can I help you today?"

# Synthesize speech to a temporary WAV file
with tempfile.NamedTemporaryFile(delete=False, suffix=".wav") as fp:
    tts.tts_to_file(text=text, file_path=fp.name, speaker="p230")  # Using speaker p230 from VCTK dataset

    # Read and play back audio
    sr, data = scipy.io.wavfile.read(fp.name)
    sd.play(data, samplerate=sr)
    sd.wait()

    # Clean up
    os.remove(fp.name)

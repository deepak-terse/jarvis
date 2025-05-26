import logging
import sys

from sensors.mic import listen
from sensors.stt import transcribe
from actuators.speaker import respond

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(message)s',
    handlers=[
        logging.FileHandler('/tmp/jarvis.log'),
        logging.StreamHandler(sys.stdout)
    ]
)

def main():
    logging.info("🧠 Jarvis started. Say 'exit' to quit.")

    while True:
        try:
            audio = listen()
            text = transcribe(audio)
            respond(text)
            logging.info(f"User said: {text}")

            if text.lower() in ("exit", "quit", "stop"):
                respond("Goodbye!")
                logging.info("👋 Exiting Jarvis loop.")
                break

        except Exception as e:
            logging.error(f"❌ Error occurred: {e}")
            respond("Something went wrong.")

if __name__ == "__main__":
    main()

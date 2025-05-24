import time
import logging
import sys

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
    while True:
        logging.info("Hello, World!")
        time.sleep(60)

if __name__ == "__main__":
    main()
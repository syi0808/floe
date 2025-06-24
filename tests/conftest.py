import logging
import pytest

@pytest.fixture(autouse=True)
def _configure_logging():
    logging.basicConfig(level=logging.WARNING)
    yield
    for handler in logging.root.handlers[:]:
        logging.root.removeHandler(handler)

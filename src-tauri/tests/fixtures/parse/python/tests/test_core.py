from app import models
from app.core import run


def test_run():
    assert run(models.User("x"))

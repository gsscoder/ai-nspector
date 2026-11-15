import os
import app.util
import requests
from app.models import User
from . import helpers


def run(user: User) -> str:
    return helpers.shout(app.util.slug(user.name))

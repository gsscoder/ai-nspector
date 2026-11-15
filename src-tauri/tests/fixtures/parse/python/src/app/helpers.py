from .util import slug as make_slug


def shout(text):
    return make_slug(text).upper()

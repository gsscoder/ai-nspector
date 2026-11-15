from dataclasses import dataclass


@dataclass
class User:
    name: str


class Admin(User):
    pass

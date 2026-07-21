import os
import pkg.mod_a
from pkg.mod_b import b


def run():
    return pkg.mod_a.a() + b() + (1 if os.name else 0)

import os
import pkg.mod_a


def run():
    return pkg.mod_a.a() + (1 if os.name else 0)

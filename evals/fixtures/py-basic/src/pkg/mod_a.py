from pkg import mod_b
import numpy


def a():
    return mod_b.b() + (0 if numpy else 1)

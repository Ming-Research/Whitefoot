#!/usr/bin/env python3
"""Select sequential emission through the unchanged formal performance Makefile."""
import os
import sys

arguments = sys.argv[1:]
if os.environ["PAR_SUITE_ARM"] == "seq":
    arguments = [arg for arg in arguments if arg != "--par"]
os.execv(os.environ["PAR_SUITE_WFC"], [os.environ["PAR_SUITE_WFC"], *arguments])

import os
import sys

from ipykernel.kernelapp import IPKernelApp

from .kernel import FermiumKernel

IPKernelApp.launch_instance(kernel_class=FermiumKernel)
sys.stdout.flush()
sys.stderr.flush()
os._exit(0)     # skip interpreter teardown of the JIT engines (see cli.entry)

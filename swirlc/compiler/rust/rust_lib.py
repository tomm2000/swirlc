import os
import shutil


def build_rust_lib(output_dir):
    current_folder = os.path.dirname(__file__)
    current_folder += "/lib"

    shutil.copytree(current_folder, output_dir, dirs_exist_ok=True)
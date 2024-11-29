import os
import shutil


def build_rust_lib(output_dir):
    # get the relative path of the current folder
    current_folder = os.path.dirname(__file__)
    current_folder += "/lib"

    # copy the folder to ./
    shutil.copytree(current_folder, output_dir, dirs_exist_ok=True)
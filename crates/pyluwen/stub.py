# Extremely heavily inspired by https://github.com/huggingface/tokenizers/blob/main/bindings/python/stub.py

import argparse
import inspect
from pathlib import Path

INDENT = " " * 4
GENERATED_COMMENT = "# Generated content DO NOT EDIT\n"

def do_indent(text: str, indent: str):
    return text.replace("\n", f"\n{indent}")

def function(obj, indent, text_signature=None):
    if text_signature is None:
        text_signature = obj.__text_signature__.replace("$self", "self")
    string = ""
    string += f"{indent}def {obj.__name__}{text_signature}:\n"
    indent += INDENT
    string += f'{indent}"""\n'
    if obj.__doc__ is not None and len(obj.__doc__) > 0:
        string += f"{indent}{do_indent(obj.__doc__, indent)}\n"
    string += f'{indent}"""\n'
    string += f"{indent}pass\n"
    string += "\n"
    return string

def member_sort(member):
    if inspect.isclass(member):
        value = 10 + len(inspect.getmro(member))
    else:
        value = 1
    return value

def fn_predicate(obj):
    value = inspect.ismethoddescriptor(obj) or inspect.isbuiltin(obj)
    if value:
        return obj.__text_signature__ and not obj.__name__.startswith("_")
    if inspect.isgetsetdescriptor(obj):
        return obj.__doc__ is not None and not obj.__name__.startswith("_")
    return False

def get_module_members(module):
    members = [
        member
        for name, member in inspect.getmembers(module)
        if not name.startswith("_") and not inspect.ismodule(member)
    ]
    members.sort(key=member_sort)
    return members


def pyi_file(obj, indent=""):
    string = ""
    if inspect.ismodule(obj):
        string += GENERATED_COMMENT
        members = get_module_members(obj)
        for member in members:
            string += pyi_file(member, indent)

    elif inspect.isclass(obj):
        indent += INDENT
        mro = inspect.getmro(obj)
        if len(mro) > 2:
            inherit = f"({mro[1].__name__})"
        else:
            inherit = ""
        string += f"class {obj.__name__}{inherit}:\n"

        body = ""
        if obj.__doc__ is not None and len(obj.__doc__) > 0:
            body += f'{indent}"""\n{indent}{do_indent(obj.__doc__, indent)}\n{indent}"""\n'

        fns = inspect.getmembers(obj, fn_predicate)

        # Init
        if obj.__text_signature__ is not None:
            sig = obj.__text_signature__
            if not sig.startswith("(self"):
                sig = sig.replace("(", "(self, ")
            body += f"{indent}def __init__{sig}:\n"
            body += f"{indent+INDENT}pass\n"
            body += "\n"

        for (name, fn) in fns:
            body += pyi_file(fn, indent=indent)

        if len(body) > 0:
            body += f"{indent}pass\n"

        string += body
        if not string.endswith("\n\n"):
            string += "\n"

    elif inspect.isbuiltin(obj):
        string += f"{indent}@staticmethod\n"
        string += function(obj, indent)

    elif inspect.ismethoddescriptor(obj):
        string += function(obj, indent)

    elif inspect.isgetsetdescriptor(obj):
        string += f"{indent}@property\n"
        string += function(obj, indent, text_signature="(self)")
    else:
        raise Exception(f"Object {obj} is not supported")
    return string


def py_file(module, origin):
    members = get_module_members(module)

    string = GENERATED_COMMENT
    string += f"from .. import {origin}\n"
    string += "\n"
    for member in members:
        name = member.__name__
        string += f"{name} = {origin}.{name}\n"
    return string


# def do_black(content, is_pyi):
#     mode = black.Mode(
#         target_versions={black.TargetVersion.PY35},
#         line_length=119,
#         is_pyi=is_pyi,
#         string_normalization=True,
#         experimental_string_processing=False,
#     )
#     try:
#         return black.format_file_contents(content, fast=True, mode=mode)
#     except black.NothingChanged:
#         return content


def write(module, pyi_filename, check=False):
    submodules = [(name, member) for name, member in inspect.getmembers(module) if inspect.ismodule(member)]

    pyi_content = pyi_file(module)
    with open(pyi_filename, "w") as f:
        f.write(pyi_content)

    assert len(submodules) == 0, "There are now submodules for pyluwen, you should extend this to support generating .pyi files for them"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")

    args = parser.parse_args()
    from pyluwen import pyluwen

    write(pyluwen, Path(__file__).parent.joinpath("pyluwen.pyi"), check=args.check)

if __name__ == "__main__":
    main()

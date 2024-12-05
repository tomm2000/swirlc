from swirlc.compiler.default import DefaultTarget
from swirlc.compiler.schema import SchemaTarget
from swirlc.compiler.rust.target import RustTarget

targets = {
    "default": DefaultTarget,
    "rust": RustTarget,
    "schema": SchemaTarget,
}

"""Small TOML edits; all parsing and file writes use the shared Rust core."""
from ._native import (
    ArgumentError,
    ConflictError,
    DocumentError,
    PatchError,
    TransactionError,
    UnsupportedError,
    patch,
    patch_file,
)

__all__ = [
    "patch", "patch_file", "PatchError", "ArgumentError", "DocumentError",
    "ConflictError", "UnsupportedError", "TransactionError",
]

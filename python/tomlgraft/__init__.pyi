from os import PathLike
from typing import Any, Literal, TypedDict

Kind = Literal["toml", "pyproject", "script", "lock"]

class PatchResult(TypedDict):
    output: bytes
    report: dict[str, Any]

class PatchError(Exception):
    code: str
    operation: int | None
    committed: bool
    durability: str
    exit_code: int
    report: dict[str, Any]
class ArgumentError(PatchError): ...
class DocumentError(PatchError): ...
class ConflictError(PatchError): ...
class UnsupportedError(PatchError): ...
class TransactionError(PatchError): ...

def patch(source: bytes, *, kind: Kind, operations: list[dict[str, Any]], create_script_block: bool = False) -> PatchResult: ...
def patch_file(path: str | PathLike[str], operations: list[dict[str, Any]], *, kind: Kind | None = None, dry_run: bool = False, create_script_block: bool = False) -> dict[str, Any]: ...

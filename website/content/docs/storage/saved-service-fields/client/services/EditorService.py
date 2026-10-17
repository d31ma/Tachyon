from typing import Annotated


@service
class EditorService:
    draft: Annotated[str, session_state] = ''
    theme: Annotated[str, local_state] = 'calm'

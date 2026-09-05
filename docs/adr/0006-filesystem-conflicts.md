# ADR 0006: User-owned files and explicit conflicts

The Library can be edited by other tools. Native commands compare the indexed hash with
the current disk hash before writing. If either changed, both local and disk content are
stored and the UI asks the user which buffer to keep. Silent overwrite is prohibited.

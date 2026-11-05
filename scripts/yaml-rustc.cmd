@echo off
rem Windows entry point for the rustc diagnostic wrapper: cargo spawns
rem RUSTC_WRAPPER via CreateProcess, which cannot execute a .py file
rem directly (no shebang support), so route through the interpreter.
python "%~dp0yaml-rustc.py" %*

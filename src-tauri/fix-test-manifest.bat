@echo off
REM Script para embeber el manifiesto de Windows en los ejecutables de test
REM Esto es necesario porque los tests de Tauri requieren comctl32 v6 para TaskDialogIndirect

setlocal enabledelayedexpansion

set MT_EXE="C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\mt.exe"
set MANIFEST_FILE=%~dp0tests.manifest
set DEPS_DIR=%~dp0target\debug\deps

REM Verificar que mt.exe existe
if not exist %MT_EXE% (
    echo ERROR: mt.exe no encontrado en %MT_EXE%
    echo Instala Windows SDK o ajusta la ruta en este script
    exit /b 1
)

REM Verificar que el manifiesto existe
if not exist "%MANIFEST_FILE%" (
    echo ERROR: tests.manifest no encontrado en %MANIFEST_FILE%
    exit /b 1
)

echo Buscando ejecutables de test en %DEPS_DIR%...

REM Buscar ejecutables de test (traductor_desktop_lib-*.exe)
for %%f in ("%DEPS_DIR%\traductor_desktop_lib-*.exe") do (
    echo Procesando: %%f
    %MT_EXE% -manifest "%MANIFEST_FILE%" -outputresource:"%%f";#1
    if !errorlevel! equ 0 (
        echo   OK: Manifiesto embebido correctamente
    ) else (
        echo   ERROR: No se pudo embeber el manifiesto
    )
)

echo.
echo Completado. Ahora puedes ejecutar los tests con: cargo test

#!/bin/bash
# Move to build folder
cd target/x86_64-pc-windows-gnu/release/
# Copy all required DLLs from the MinGW system path
cp /usr/x86_64-w64-mingw32/bin/*.dll .
# Zip it up
zip -r rui-windows.zip . -i "*.exe" "*.dll"
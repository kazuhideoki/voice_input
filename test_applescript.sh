#!/bin/bash

echo "Testing AppleScript keystroke command..."
echo "Please open a text editor and place cursor in a text field"
sleep 3

# Test 1: Simple text
echo "Test 1: Simple text"
osascript -e 'tell application "System Events" to keystroke "Hello"'
sleep 1

# Test 2: Text with space
echo "Test 2: Text with space"
osascript -e 'tell application "System Events" to keystroke "Hello World"'
sleep 1

# Test 3: Text with special chars
echo "Test 3: Text with quotes"
osascript -e 'tell application "System Events" to keystroke "Hello \"World\""'
#!/bin/bash

echo "Testing Japanese text with AppleScript..."
echo "Please open a text editor and place cursor in a text field"
sleep 3

# Test 1: Direct Japanese text
echo "Test 1: Direct Japanese text"
osascript -e 'tell application "System Events" to keystroke "こんにちは"'
sleep 2

# Test 2: Using text variable
echo "Test 2: Using text variable"
osascript -e 'set myText to "こんにちは"
tell application "System Events" to keystroke myText'
sleep 2

# Test 3: Using key code approach (alternative)
echo "Test 3: Alternative approach with clipboard"
echo -n "テストテキスト" | pbcopy
osascript -e 'tell application "System Events" to keystroke "v" using command down'
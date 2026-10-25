-- Target the native control by its accessibility identifier, never its
-- localized label or position. The action worker bounds and cancels this script.
with timeout of 3 seconds
tell application "System Events"
    if not UI elements enabled then error "Allow ACA Studio in Accessibility and allow it to control System Events before using On-screen keyboard."
    set previousPid to unix id of first application process whose frontmost is true
end tell
do shell script "/usr/bin/open 'x-apple.systempreferences:com.apple.preference.universalaccess?Keyboard'"

set keyboardControl to missing value
repeat 40 times
    tell application "System Events"
        if exists application process "System Settings" then
            tell application process "System Settings"
                if exists window 1 then
                    repeat with candidate in (entire contents of window 1)
                        try
                            if value of attribute "AXIdentifier" of candidate is "AX_VIRTUAL_KEYBOARD" then
                                set keyboardControl to contents of candidate
                                exit repeat
                            end if
                        end try
                    end repeat
                end if
            end tell
        end if
    end tell
    if keyboardControl is not missing value then exit repeat
    delay 0.1
end repeat
if keyboardControl is missing value then error "On-screen keyboard control unavailable. Open System Settings > Accessibility > Keyboard and enable Accessibility Keyboard."

tell application "System Events"
    set currentValue to value of keyboardControl
    if currentValue is not 0 and currentValue is not 1 then error "Could not determine the Accessibility Keyboard state. No control was changed."
    if currentValue is 0 then click keyboardControl
end tell
repeat 30 times
    tell application "System Events"
        if value of keyboardControl is 1 then
            try
                set frontmost of first application process whose unix id is previousPid to true
            end try
            return "Accessibility Keyboard enabled"
        end if
    end tell
    delay 0.1
end repeat
error "macOS did not enable the Accessibility Keyboard. Check System Settings > Accessibility > Keyboard."
end timeout

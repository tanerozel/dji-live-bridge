; Registers the DirectShow camera filter so apps such as TikTok LIVE Studio,
; OBS and Zoom list "DJI Live Bridge Camera" among their cameras.
;
; The filter is a 64-bit DLL, but NSIS itself runs as a 32-bit process, so its
; own regsvr32 would be the 32-bit one and could not register it. "Sysnative"
; is the alias that gives a 32-bit process the real 64-bit System32, which is
; where the right regsvr32 lives. On a 32-bit Windows the alias does not exist
; and System32 is already correct.
;
; Registration writes under HKCR, so it needs the elevation the per-machine
; installer already has. A failure is reported but never aborts the install:
; the app still streams, it only loses the camera, and it says so in its UI.
;
; These macros are expanded inside the installer's own sections, so they use
; plain instructions and the scratch registers only — a "Var" declaration is
; not valid there, and nothing here may depend on headers the template may or
; may not have included.

!macro NSIS_HOOK_POSTINSTALL
  Push $0
  Push $1
  StrCpy $0 "$WINDIR\System32\regsvr32.exe"
  IfFileExists "$WINDIR\Sysnative\regsvr32.exe" 0 dji_register_run
  StrCpy $0 "$WINDIR\Sysnative\regsvr32.exe"
dji_register_run:
  DetailPrint "Registering the DJI Live Bridge camera..."
  ExecWait '"$0" /s "$INSTDIR\resources\dji_virtual_camera.dll"' $1
  StrCmp $1 "0" dji_register_done 0
  DetailPrint "The camera could not be registered (code $1). Streaming still works; use OBS Virtual Camera if you need a camera device."
dji_register_done:
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Push $0
  StrCpy $0 "$WINDIR\System32\regsvr32.exe"
  IfFileExists "$WINDIR\Sysnative\regsvr32.exe" 0 dji_unregister_run
  StrCpy $0 "$WINDIR\Sysnative\regsvr32.exe"
dji_unregister_run:
  DetailPrint "Removing the DJI Live Bridge camera..."
  ; Unregister before the file is deleted, or the CLSID would be left behind
  ; pointing at a DLL that no longer exists.
  ExecWait '"$0" /s /u "$INSTDIR\resources\dji_virtual_camera.dll"'
  Pop $0
!macroend

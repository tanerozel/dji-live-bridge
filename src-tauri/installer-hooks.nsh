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
; the app still streams, it only loses the camera, and it says so in the UI.

!macro RegsvrPath OutVar
  ${If} ${FileExists} "$WINDIR\Sysnative\regsvr32.exe"
    StrCpy ${OutVar} "$WINDIR\Sysnative\regsvr32.exe"
  ${Else}
    StrCpy ${OutVar} "$WINDIR\System32\regsvr32.exe"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Var /GLOBAL DjiRegsvr
  Var /GLOBAL DjiRegResult
  !insertmacro RegsvrPath $DjiRegsvr
  DetailPrint "Registering the DJI Live Bridge camera..."
  ExecWait '"$DjiRegsvr" /s "$INSTDIR\resources\dji_virtual_camera.dll"' $DjiRegResult
  ${If} $DjiRegResult != 0
    DetailPrint "The camera could not be registered (code $DjiRegResult). Streaming still works; \
      use OBS Virtual Camera if you need a camera device."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Var /GLOBAL DjiUnregsvr
  !insertmacro RegsvrPath $DjiUnregsvr
  DetailPrint "Removing the DJI Live Bridge camera..."
  ; Unregister before the file is deleted, or its CLSID would be left behind
  ; pointing at a DLL that no longer exists.
  ExecWait '"$DjiUnregsvr" /s /u "$INSTDIR\resources\dji_virtual_camera.dll"'
!macroend

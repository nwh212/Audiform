; SPDX-License-Identifier: GPL-3.0-or-later
; Copyright (c) The Audiform contributors.
; This file is part of Audiform, based on Altered Audio Editor.
;
; Audiform never listens on the network, but some plug-ins do (licence
; checks, or plug-ins of one maker finding each other), and Windows then
; asks whether Audiform may use public networks. This rule answers no
; ahead of time, so the question never comes up. Traffic that stays on
; this computer is not filtered, so plug-ins still reach each other.
;
; Firewall rules need administrator rights. A per-user install has none,
; so when the plain attempt fails Windows is asked once (an
; administrator prompt) to add the rule.

!define AUDIFORM_RULE 'name="Audiform (no incoming network)"'

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule ${AUDIFORM_RULE}'
  Pop $0
  nsExec::Exec 'netsh advfirewall firewall add rule ${AUDIFORM_RULE} dir=in action=block profile=any program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes'
  Pop $0
  ${If} $0 != 0
    ExecShell "runas" "netsh" 'advfirewall firewall add rule ${AUDIFORM_RULE} dir=in action=block profile=any program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes' SW_HIDE
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule ${AUDIFORM_RULE}'
  Pop $0
  ${If} $0 != 0
    ExecShell "runas" "netsh" 'advfirewall firewall delete rule ${AUDIFORM_RULE}' SW_HIDE
  ${EndIf}
!macroend

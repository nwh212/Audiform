; SPDX-License-Identifier: GPL-3.0-or-later
; Copyright (c) The Audiform contributors.
; This file is part of Audiform, based on Altered Audio Editor.
;
; Audiform never listens on the network, but some plug-ins do (licence
; checks, or plug-ins of one maker finding each other), and Windows then
; asks whether Audiform may use public networks. This rule answers no
; ahead of time, so the question never comes up. Traffic that stays on
; this computer is not filtered, so plug-ins still reach each other.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="Audiform (no incoming network)"'
  nsExec::Exec 'netsh advfirewall firewall add rule name="Audiform (no incoming network)" dir=in action=block profile=any program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="Audiform (no incoming network)"'
!macroend

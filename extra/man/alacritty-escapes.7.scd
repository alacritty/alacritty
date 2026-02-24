ALACRITTY-ESCAPES(7)

# Alacritty Escape Sequence Support

This list includes all escape sequences Alacritty currently supports.

# Legend

The available statuses are *PARTIAL*, *IMPLEMENTED* and *REJECTED*. While a
status of *PARTIAL* means there is still work left to be done, a status of
*IMPLEMENTED* for something partially implemented means all other features were
rejected.

All whitespace in escape sequences is solely for formatting purposes and all
relevant spaces are denoted as *SP*. The escape parameters are omitted for
brevity.

# ESC codes - ESC

[[ *ESCAPE*
:[ *STATUS*
:[ *NOTE*
|  _ESC (_
:  Implemented
:[ Only charsets *B* and *0* are supported
|  _ESC )_
:  Implemented
:[ Only charsets *B* and *0* are supported
|  _ESC \*_
:  Implemented
:[ Only charsets *B* and *0* are supported
|  _ESC +_
:  Implemented
:[ Only charsets *B* and *0* are supported
|  _ESC =_
:  Implemented
:[
|  _ESC >_
:  Implemented
:[
|  _ESC 7_
:  Implemented
:[
|  _ESC 8_
:  Implemented
:[
|  _ESC # 8_
:  Implemented
:[
|  _ESC D_
:  Implemented
:[
|  _ESC E_
:  Implemented
:[
|  _ESC H_
:  Implemented
:[
|  _ESC M_
:  Implemented
:[
|  _ESC Z_
:  Implemented
:[

# CSI (Control Sequence Introducer) - ESC [

[[ *ESCAPE*
:[ *STATUS*
:[ *NOTE*
|  _CSI \`_
:  Implemented
:[
|  _CSI @_
:  Implemented
:[
|  _CSI A_
:  Implemented
:[
|  _CSI a_
:  Implemented
:[
|  _CSI B_
:  Implemented
:[
|  _CSI b_
:  Implemented
:[
|  _CSI C_
:  Implemented
:[
|  _CSI c_
:  Implemented
:[
|  _CSI D_
:  Implemented
:[
|  _CSI d_
:  Implemented
:[
|  _CSI E_
:  Implemented
:[
|  _CSI e_
:  Implemented
:[
|  _CSI F_
:  Implemented
:[
|  _CSI f_
:  Implemented
:[
|  _CSI G_
:  Implemented
:[
|  _CSI g_
:  Implemented
:[
|  _CSI H_
:  Implemented
:[
|  _CSI h_
:  Partial
:[ Modes: *4*, *20*
|  _CSI ? h_
:  Partial
:[ Modes: *1*, *3*, *6*, *7*, *12*, *25*, *1000*, *1002*, *1004*, *1005*, *1006*, *1007*, *1042*, *1049*, *2004*, *2026*
|  _CSI I_
:  Implemented
:[
|  _CSI J_
:  Implemented
:[
|  _CSI K_
:  Implemented
:[
|  _CSI L_
:  Implemented
:[
|  _CSI l_
:  Partial
:[ See _CSI h_ for supported modes
|  _CSI ? l_
:  Partial
:[ See _CSI ? h_ for supported modes
|  _CSI M_
:  Implemented
:[
|  _CSI m_
:  Implemented
:[ Parameters: *0*-*9*, *21*-*25*, *27*-*49*, *58*, *59*, *90*-*97*, *100*-*107*
|  _CSI m_
:  Rejected
:[ Parameters: *11*-*19*, *51*-*55*
|  _CSI n_
:  Implemented
:[
|  _CSI P_
:  Implemented
:[
|  _CSI $ p_
:  Implemented
:[
|  _CSI ? $ p_
:  Implemented
:[
|  _CSI SP q_
:  Implemented
:[
|  _CSI r_
:  Implemented
:[
|  _CSI S_
:  Implemented
:[
|  _CSI s_
:  Implemented
:[
|  _CSI T_
:  Implemented
:[
|  _CSI t_
:  Partial
:[ Parameters: *22*, *23*
|  _CSI t_
:  Rejected
:[ Parameters: *1*-*13*, *15*, *19*-*21*, *24*
|  _CSI u_
:  Implemented
:[
|  _CSI ? u_
:  Implemented
:[
|  _CSI = u_
:  Implemented
:[
|  _CSI < u_
:  Implemented
:[
|  _CSI > u_
:  Implemented
:[
|  _CSI X_
:  Implemented
:[
|  _CSI Z_
:  Implemented
:[

# OSC (Operating System Command) - ESC ]

[[ *ESCAPE*
:[ *STATUS*
:[ *NOTE*
|  _OSC 0_
:  Implemented
:[ Icon names are not supported
|  _OSC 1_
:  Rejected
:[ Icon names are not supported
|  _OSC 2_
:  Implemented
:[
|  _OSC 4_
:  Implemented
:[
|  _OSC 8_
:  Implemented
:[
|  _OSC 10_
:  Implemented
:[
|  _OSC 11_
:  Implemented
:[
|  _OSC 12_
:  Implemented
:[
|  _OSC 50_
:  Implemented
:[ Only *CursorShape* is supported
|  _OSC 52_
:  Implemented
:[ Only *clipboard* and *primary selection* supported
|  _OSC 104_
:  Implemented
:[
|  _OSC 110_
:  Implemented
:[
|  _OSC 111_
:  Implemented
:[
|  _OSC 112_
:  Implemented
:[

# DCS (Device Control String) - ESC P

[[ *ESCAPE*
:[ *STATUS*
:[ *NOTE*
|  _DCS = s_
:  Rejected
:[ _CSI ? 2026 h/l_ is used instead

# SEE ALSO

*alacritty*(1), *alacritty-msg*(1), *alacritty*(5), *alacritty-bindings*(5)

# BUGS

Found a bug? Please report it at _https://github.com/alacritty/alacritty/issues_.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>

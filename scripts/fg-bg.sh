#!/bin/bash

printf "Fg=Black,     Bg=Background          \e[30;49mTEST\e[m\n"
printf "Fg=Black,     Bg=Black               \e[30;40mTEST\e[m\n"
printf "Fg=Foreground,Bg=Background          \e[39;49mTEST\e[m\n"
printf "Fg=Foreground,Bg=Black               \e[39;40mTEST\e[m\n"
printf "Fg=Foreground,Bg=White               \e[39;47mTEST\e[m\n"
printf "Fg=White,     Bg=Foreground          \e[37;39mTEST\e[m\n"
printf "Fg=Black,     Bg=Background, Inverse \e[7;30;49mTEST\e[m\n"
printf "Fg=Black,     Bg=Black,      Inverse \e[7;30;40mTEST\e[m\n"
printf "Fg=Foreground,Bg=Background, Inverse \e[7;39;49mTEST\e[m\n"
printf "Fg=Foreground,Bg=Black,      Inverse \e[7;39;40mTEST\e[m\n"
printf "Fg=Foreground,Bg=White,      Inverse \e[7;39;47mTEST\e[m\n"
printf "Fg=White,     Bg=Foreground, Inverse \e[7;37;39mTEST\e[m\n"

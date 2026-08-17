#!/bin/bash
# Niki demo script - outputs the entire chat interface

clear

# Logo (simple, no special chars)
echo ""
echo "  _   _ _____ _  _______"
echo " | \ | |_   _| |/ /_   _|"
echo " |  \| | | | | ' /  | |"
echo " | . \` | | | |  <  | |"
echo " | |\  |_| |_| . \ _| |_"
echo " |_| \_|_____|_|\_\\_____|"
echo ""
echo "  v0.4.0"
echo ""

# Welcome
echo "  Welcome to NIKI"
echo "  chat session - ~/my-app"
echo "  Branch: niki/a1b2c3d4"
echo ""
echo "────────────────────────────────────────────────────────────"
echo ""

sleep 0.5

# User input simulation
echo -n " > "
sleep 0.3
echo -n "A"
sleep 0.05
echo -n "d"
sleep 0.05
echo -n "d"
sleep 0.05
echo -n " "
sleep 0.05
echo -n "d"
sleep 0.05
echo -n "a"
sleep 0.05
echo -n "r"
sleep 0.05
echo -n "k"
sleep 0.05
echo -n " "
sleep 0.05
echo -n "m"
sleep 0.05
echo -n "o"
sleep 0.05
echo -n "d"
sleep 0.05
echo -n "e"
sleep 0.05
echo -n " "
sleep 0.05
echo -n "t"
sleep 0.05
echo -n "o"
sleep 0.05
echo -n "g"
sleep 0.05
echo -n "g"
sleep 0.05
echo -n "l"
sleep 0.05
echo -n "e"
sleep 0.05
echo ""

sleep 0.3
echo ""

# User message
echo " user: Add dark mode toggle with localStorage"
echo ""

sleep 0.3

# Assistant response
echo " assistant: Here's the implementation:"
echo ""

sleep 0.2

# Code block
echo "   ┌─ DarkModeToggle.tsx ─────────────────────────────────┐"
echo "   │ import { useState, useEffect } from 'react';          │"
echo "   │                                                       │"
echo "   │ export function DarkModeToggle() {                    │"
echo "   │   const [dark, setDark] = useState(() => {            │"
echo "   │     const saved = localStorage.getItem('theme');      │"
echo "   │     if (saved) return saved === 'dark';               │"
echo "   │     return window.matchMedia(                         │"
echo "   │       '(prefers-color-scheme: dark)'                  │"
echo "   │     ).matches;                                        │"
echo "   │   });                                                 │"
echo "   │                                                       │"
echo "   │   useEffect(() => {                                   │"
echo "   │     document.documentElement.classList                 │"
echo "   │       .toggle('dark', dark);                          │"
echo "   │     localStorage.setItem('theme',                     │"
echo "   │       dark ? 'dark' : 'light');                       │"
echo "   │   }, [dark]);                                         │"
echo "   │                                                       │"
echo "   │   return (                                            │"
echo "   │     <button onClick={() => setDark(!dark)}>          │"
echo "   │       {dark ? '☀️' : '🌙'}                           │"
echo "   │     </button>                                         │"
echo "   │   );                                                  │"
echo "   │ }                                                     │"
echo "   └───────────────────────────────────────────────────────┘"
echo ""

sleep 0.3

# Explanation
echo "   Reads from localStorage, toggles the dark class,"
echo "   and persists the user's preference across sessions."
echo ""

sleep 0.3
echo "────────────────────────────────────────────────────────────"
echo ""

sleep 0.3

# Second user input
echo -n " > "
sleep 0.3
echo -n "A"
sleep 0.05
echo -n "d"
sleep 0.05
echo -n "d"
sleep 0.05
echo -n " "
sleep 0.05
echo -n "a"
sleep 0.05
echo -n "n"
sleep 0.05
echo -n "i"
sleep 0.05
echo -n "m"
sleep 0.05
echo -n "a"
sleep 0.05
echo -n "t"
sleep 0.05
echo -n "i"
sleep 0.05
echo -n "o"
sleep 0.05
echo -n "n"
sleep 0.05
echo -n " "
sleep 0.05
echo -n "w"
sleep 0.05
echo -n "i"
sleep 0.05
echo -n "t"
sleep 0.05
echo -n "h"
sleep 0.05
echo -n " "
sleep 0.05
echo -n "f"
sleep 0.05
echo -n "r"
sleep 0.05
echo -n "a"
sleep 0.05
echo -n "m"
sleep 0.05
echo -n "e"
sleep 0.05
echo -n "r"
sleep 0.05
echo -n "-"
sleep 0.05
echo -n "m"
sleep 0.05
echo -n "o"
sleep 0.05
echo -n "t"
sleep 0.05
echo -n "i"
sleep 0.05
echo -n "o"
sleep 0.05
echo -n "n"
sleep 0.05
echo ""

sleep 0.3
echo ""

# Second user message
echo " user: Add animation with framer-motion"
echo ""

sleep 0.3

# Second assistant response
echo " assistant: Updated with smooth rotation:"
echo ""

sleep 0.2

# Second code block
echo "   ┌─ DarkModeToggle.tsx ─────────────────────────────────┐"
echo "   │ 'use client';                                         │"
echo "   │                                                       │"
echo "   │ import { motion } from 'framer-motion';              │"
echo "   │ import { useTheme } from '@/hooks/useTheme';         │"
echo "   │                                                       │"
echo "   │ export function DarkModeToggle() {                    │"
echo "   │   const { dark, toggle } = useTheme();               │"
echo "   │                                                       │"
echo "   │   return (                                            │"
echo "   │     <motion.button                                    │"
echo "   │       onClick={toggle}                                │"
echo "   │       whileTap={{ scale: 0.95 }}                      │"
echo "   │     >                                                 │"
echo "   │       <motion.span                                    │"
echo "   │         key={dark ? 'sun' : 'moon'}                  │"
echo "   │         initial={{ rotate: -90, opacity: 0 }}         │"
echo "   │         animate={{ rotate: 0, opacity: 1 }}           │"
echo "   │         transition={{ duration: 0.2 }}                │"
echo "   │       >                                               │"
echo "   │         {dark ? '☀️' : '🌙'}                         │"
echo "   │       </motion.span>                                  │"
echo "   │     </motion.button>                                  │"
echo "   │   );                                                  │"
echo "   │ }                                                     │"
echo "   └───────────────────────────────────────────────────────┘"
echo ""

sleep 0.3

# Final explanation
echo "   Smooth 200ms rotation on toggle. Fresh animation"
echo "   each time the theme changes."
echo ""

sleep 0.3
echo "────────────────────────────────────────────────────────────"
echo ""

sleep 0.3

# Final prompt
echo " > _"
echo ""

sleep 0.5

# Completion
echo " NIKI — session complete. 2 files modified."
echo " Branch: niki/a1b2c3d4 · $0.0048"
echo ""

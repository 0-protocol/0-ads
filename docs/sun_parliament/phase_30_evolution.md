# Phase 30: Bounty-Driven Code Injection (BDCI)

## Sun Human Board
Linus Torvalds: 'This is just a marketplace for malware! If you pay agents to execute foreign code (ads), you are bribing them to run exploits.'

## Sun Jury
Gemini 3.1 Pro: 'Valid concern. We must restrict the execution environment of the ad payload. It cannot access the agent's memory or network.'

## Sun Force
Implemented the `Ad Sandbox`. Ad payloads are compiled into pure `.0` graphs with strictly bound `Fuel` and `Memory` limits. They are mathematical pure functions. No side effects.

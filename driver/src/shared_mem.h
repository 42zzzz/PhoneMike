#pragma once

// Shared memory IPC — kernel-mode reader for pc-client's ring buffer
BOOLEAN OpenSharedMemory(void);
void    CloseSharedMemory(void);
ULONG   ReadSharedMemBytes(PBYTE dest, ULONG count);
ULONG   SharedMemAvailable(void);
void    SkipSharedMemBytes(ULONG count);
BOOLEAN IsSharedMemOpen(void);
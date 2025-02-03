### Configuration

# Path to the file in which to write the transferred data
$OUTPUT_PATH = "favicon.ico"

# Path to the supported backend DLLs:
# - Citrix (C:\Program Files\Citrix\ICAService\wfapi64.dll)
#   The 32-bit version (wfapi.dll) should also work
$WFAPI_PATH = "wfapi64.dll"
# - Horizon (TODO)
$VDPAPI_PATH = "vdp_rdpvcbridge.dll"
# - RDP (TODO)
$WTSAPI_PATH = "wtsapi32.dll"

# Name of the virtual channel to open (max 7 characters)
$VC_NAME = "SOXY"

# Name of the soxy service we are implementing
$SOXY_SVC_NAME = "stage0"

# Timeout (in ms) for virtual channel read operations
# This is purposeful long to allow users to launch the transfer from the
# frontend after running the script
[UInt32] $VC_TIMEOUT_MILLISECONDS = 30000

# Maximum number of bytes which can be read at at time from the virtual channel
[UInt32] $SOXY_CHUNK_MAX_LEN = 1600

# Error handling is performed manually in the script to avoid exiting with an
# unclean state (e.g. file or virtual channel still open)
$ErrorActionPreference = "Continue"

# Print verbose output instead of ignoring it
#$VerbosePreference = "Continue"

### DLL import

# Define function prototypes from the various DLLs to be able to use them here
# Type mapping: https://www.codeproject.com/Articles/9714/Win32-API-C-to-NET

# Citrix
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public enum WF_INFO_CLASS {
    WFVersion,
    WFInitialProgram,
    WFWorkingDirectory,
    WFOEMId,
    WFSessionId,
    WFUserName,
    WFWinStationName,
    WFDomainName,
    WFConnectState,
    WFClientBuildNumber,
    WFClientName,
    WFClientDirectory,
    WFClientProductId,
    WFClientHardwareId,
    WFClientAddress,
    WFClientDisplay,
    WFClientCache,
    WFClientDrives,
    WFICABufferLength,
    WFLicenseEnabler,
    RESERVED2,
    WFApplicationName,
    WFVersionEx,
    WFClientInfo,
    WFUserInfo,
    WFAppInfo,
    WFClientLatency,
    WFSessionTime,
    WFLicensingModel
};

public enum WF_VIRTUAL_CLASS {
    WFVirtualClientData
};

public static class WFApi {
    public static IntPtr WF_CURRENT_SERVER_HANDLE = IntPtr.Zero;
    public static uint WF_CURRENT_SESSION = 0xffffffff;

    [DllImport(@"$WFAPI_PATH", CharSet=CharSet.Unicode, CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern Boolean WFQuerySessionInformation(
        IntPtr        hServer,
        uint          SessionId,
        WF_INFO_CLASS WFInfoClass,
        out string    ppBuffer,
        out UInt32    pBytesReturned
    );

    [DllImport(@"$WFAPI_PATH", CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern IntPtr WFVirtualChannelOpen(
        IntPtr hServer,
        UInt32 SessionId,
        string pVirtualName
    );

    [DllImport(@"$WFAPI_PATH", CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern Boolean WFVirtualChannelClose(
        IntPtr hChannelHandle
    );

    [DllImport(@"$WFAPI_PATH", CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern Boolean WFVirtualChannelRead(
        IntPtr     hChannelHandle,
        UInt32     TimeOut,
        byte[]     Buffer,
        UInt32     BufferSize,
        out UInt32 pBytesRead
    );

    [DllImport(@"$WFAPI_PATH", CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern Boolean WFVirtualChannelWrite(
        IntPtr     hChannelHandle,
        byte[]     Buffer,
        UInt32     Length,
        out UInt32 pBytesWritten
    );

    [DllImport(@"$WFAPI_PATH", CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern Boolean WFVirtualChannelQuery(
        IntPtr           hChannelHandle,
        WF_VIRTUAL_CLASS VirtualClass,
        out byte[]       ppBuffer,
        out UInt32       pBytesReturned
    );

    [DllImport(@"$WFAPI_PATH", CallingConvention=CallingConvention.Winapi, SetLastError=true, ThrowOnUnmappableChar=true)]
    public static extern void WFFreeMemory(
        IntPtr pMemory
    );
}
"@

# Horizon
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class VDPApi {
}
"@

# RDP
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class WTSApi {
}
"@

# Add structures defined in icaC2 header files to access them in PowerShell
# To simplify, we only keep the entries which are used here
$cp = [CodeDom.Compiler.CompilerParameters]::new()
$cp.CompilerOptions = "/unsafe"
Add-Type -CompilerParameters $cp -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public enum SOXY_CHUNK_TYPE: byte {
    ID_START = 0,
    ID_DATA = 1,
    ID_END = 2,
};

[StructLayout(LayoutKind.Sequential, Pack=1)]
public struct SOXY_CHUNK_HEADER {
    public UInt32 ClientId;
    public SOXY_CHUNK_TYPE ChunkType;
    public UInt16 PayloadLen;
};
"@

### Helper functions

function Cleanup-Exit {
    if ($fs -and $fs.CanWrite) {
        $fs.Close()
    }
    if ($channel) {
        $ret = Close-VirtualChannel $channel
    }

    # Don't call Exit here in case this isn't run as a script
    throw "File transfer failed"
}

function ConvertTo-Hexdump {
    [OutputType([string])]
    Param (
        [Parameter(Mandatory=$true)] [Byte[]] $buf,
        [Parameter(Mandatory=$true)] [Uint32] $len
    )
    if ($len -gt 0) {
        return ($buf[0..($len - 1)] | Format-Hex) -join "`r`n"
    } else {
        return ""
    }
}

function Convert-BytesToStruct {
    [OutputType([Object[]])]
    Param(
        [Parameter(Mandatory = $true)] [Type] $type,
        [Parameter(Mandatory = $true)] [Byte[]] $buf
    )

    # Check buffer is big enough
    $structSize = [Runtime.InteropServices.Marshal]::SizeOf([type] $type)
    if ($structSize -gt $buf.Length){
        Write-Error("Buffer of size {0} for channel {1} is smaller than structure size {2}" -f $buf.Length, $VC_NAME, $structSize)
        return
    }

    $structPtr = [IntPtr]::Zero
    try {
        # Copy the buffer data to a pointer to then cast it to the structure type
        $structPtr = [Runtime.InteropServices.Marshal]::AllocHGlobal($structSize)
        [Runtime.InteropServices.Marshal]::Copy($buf, 0, $structPtr, $structSize)
        return [Runtime.InteropServices.Marshal]::PtrToStructure($structPtr, [type] $type)
    } finally {
        # Clean up allocated memory
        if ($structPtr -ne [IntPtr]::Zero) {
            [Runtime.InteropServices.Marshal]::FreeHGlobal($structPtr)
        }
    }
}

function Open-VirtualChannel {
    [OutputType([IntPtr])]
    Param (
        [Parameter(Mandatory=$true)] [string] $name
    )

    if ($name.Length -gt 7) {
        Write-Error("Failed to open channel {0}: virtual channel name cannot be longer than 8 bytes." -f $name)
        return [IntPtr]::Zero
    }

    $channel = [WFApi]::WFVirtualChannelOpen(
        [WFApi]::WF_CURRENT_SERVER_HANDLE,
        [WFApi]::WF_CURRENT_SESSION,
        $name
    )
    if ($channel -eq 0) {
        $error = [ComponentModel.Win32Exception][Runtime.InteropServices.Marshal]::GetLastWin32Error()
        Write-Error("WFVirtualChannelOpen failed for channel {0} with error {1}: {2}." -f $name, $error.NativeErrorCode, $error.Message)
    } else {
        Write-Host("Successfully opened virtual channel {0}." -f $name)
    }
    return $channel
}

function Close-VirtualChannel {
    [OutputType([Boolean])]
    Param (
        [Parameter(Mandatory=$true)] [IntPtr] $vcHandle
    )

    $ret = [WFApi]::WFVirtualChannelClose($vcHandle)
    if ($ret -eq 0) {
        $error = [ComponentModel.Win32Exception][Runtime.InteropServices.Marshal]::GetLastWin32Error()
        Write-Error("WFVirtualChannelClose failed for channel {0} with error {1}: {2}." -f $VC_NAME, $error.NativeErrorCode, $error.Message)
    } else {
        Write-Host("Successfully closed virtual channel {0}." -f $VC_NAME)
    }
    return $ret
}

function Read-VirtualChannelQuery {
    [OutputType([Boolean])]
    Param (
        [Parameter(Mandatory=$true)] [IntPtr] $vcHandle,
        [Parameter(Mandatory=$true)] [ref] $outData,
        [Parameter(Mandatory=$true)] [ref] $outLen
    )

    $ret = [WFApi]::WFVirtualChannelQuery(
        $vcHandle,
        [WF_VIRTUAL_CLASS]::WFVirtualClientData,
        $outData,
        $outLen
    )
    if ($ret -eq 0) {
        $error = [ComponentModel.Win32Exception][Runtime.InteropServices.Marshal]::GetLastWin32Error()
        Write-Error("WFVirtualChannelQuery failed with error {0}: {1}." -f $error.NativeErrorCode, $error.Message)
        if ($error.NativeErrorCode -eq 1) {
            Write-Warning("This error typically occurs when the client did not properly connect to the virtual channel. Make sure ICAC2.DLL was properly loaded by Citrix when you opened the VDA session.")
        }
    } else {
        Write-Verbose("Queried {0} bytes from channel {1}." -f $outLen.Value, $VC_NAME)
    }
    return $ret
}

function Read-Chunk {
    [OutputType([Boolean])]
    Param (
        [Parameter(Mandatory=$true)] [IntPtr] $vcHandle,
        [Parameter(Mandatory=$true)] [ref] $outHeader,
        [Parameter(Mandatory=$true)] [Byte[]] $outPayload,
        [Parameter(Mandatory=$true)] [ref] $outPayloadLen
    )

    if ($SOXY_CHUNK_MAX_LEN -gt $outPayload.Length) {
        Write-Warning("Potential buffer overflow in WFVirtualChannelRead for channel {0}: buffer of size {1} is shorter than {2}." -f $VC_NAME, $outPayload.Length, $SOXY_CHUNK_MAX_LEN)
    }

    # Read data into local variable
    $chunkBuf = [Byte[]]::new($SOXY_CHUNK_MAX_LEN)
    [UInt32] $chunkLen = 0

    $ret = [WFApi]::WFVirtualChannelRead(
        $vcHandle, $VC_TIMEOUT_MILLISECONDS, $chunkBuf, $SOXY_CHUNK_MAX_LEN, [ref] $chunkLen
    )
    if ($ret -eq 0) {
        $error = [ComponentModel.Win32Exception][Runtime.InteropServices.Marshal]::GetLastWin32Error()
        Write-Error("WFVirtualChannelRead failed with error {0}: {1}." -f $error.NativeErrorCode, $error.Message)
        return 0
    }

    # Get header from read data
    $outHeader.Value = Convert-BytesToStruct ([type] [SOXY_CHUNK_HEADER]) $chunkBuf
    $headerSize = [Runtime.InteropServices.Marshal]::SizeOf($outHeader.Value)
    Write-Verbose($outHeader.Value | Format-Table | Out-String)

    # Check payload has (at least) the expected size
    if (($header.PayloadLen + $headerSize) -gt $chunkLen) {
        Write-Error("Not enough data read in WFVirtualChannelRead: got {0}, expected {1}." -f $chunkLen, $header.PayloadLen + $headerSize)
        return 0
    }

    # Store chunk payload without the header
    $chunkBuf[$headerSize..$chunkLen].CopyTo($outPayload, 0)
    $outPayloadLen.Value = $chunkLen - $headerSize

    Write-Verbose("Read {0} bytes from channel {1}:`r`n{2}." -f $outPayloadLen.Value, $VC_NAME, (ConvertTo-Hexdump $outPayload $outPayloadLen.Value))
    return 1
}

### Virtual channel creation

# Check whether we are running in an ICA session
$pSessionInfo = "";
[UInt32] $ByteCount = 0;

$ret = [WFApi]::WFQuerySessionInformation(
    [WFApi]::WF_CURRENT_SERVER_HANDLE,
    [WFApi]::WF_CURRENT_SESSION,
    [WF_INFO_CLASS]::WFSessionId,
    [ref] $pSessionInfo,
    [ref] $ByteCount
)
if ($ret -eq 0) {
    $error = [ComponentModel.Win32Exception][Runtime.InteropServices.Marshal]::GetLastWin32Error()
    Write-Error("WFQuerySessionInformation failed with error {0}: {1}." -f $error.NativeErrorCode, $error.Message)
    Cleanup-Exit
} elseif (!$pSessionInfo) {
    Write-Error("This program must be run from within an ICA session.")
    Cleanup-Exit
}

# Open virtual channel
$channel = Open-VirtualChannel $VC_NAME
if ($channel -eq 0) {
    Write-Error("Failed to open virtual channel {0}." -f $VC_NAME)
    Cleanup-Exit
}

# Query virtual channel information to ensure the client is connected
$queryBuf = [Byte[]]::new(64)
[UInt32] $queryLen = 0

if (!(Read-VirtualChannelQuery $channel ([ref] $queryBuf) ([ref] $queryLen))) {
    Cleanup-Exit
}

### Task initiation

Write-Host("Waiting for client to connect...")

$header = New-Object SOXY_CHUNK_HEADER
$payloadBuf = [Byte[]]::new($SOXY_CHUNK_MAX_LEN)
[UInt32] $payloadLen = 0

while ($true) {
    # Wait for the client to send a chunk
    if (!(Read-Chunk $channel ([ref] $header) $payloadBuf ([ref] $payloadLen))) {
        Cleanup-Exit
    }

    # Ensure this is a START chunk...
    if ($header.ChunkType -ne [SOXY_CHUNK_TYPE]::ID_START) {
        Write-Warning("Got unexpected chunk type {0} for channel {1} (expected ID_START), ignoring." -f $header.ChunkType, $VC_NAME)
        Continue
    }

    # ... And that it targets the right service
    $serviceBuf = [Byte[]]::new($SOXY_CHUNK_MAX_LEN)
    $payloadBuf[0..$header.PayloadLen].CopyTo($serviceBuf, 0)
    $service = [Text.Encoding]::ASCII.GetString($serviceBuf).Trim("`0")
    if ($service -ne $SOXY_SVC_NAME) {
        Write-Error("Got unexpected service name {0} for channel {1} (expected {2}), ignoring." -f $service, $VC_NAME, $SOXY_SVC_NAME)
        Continue
    }

    Write-Host("Client connected with ID {0} for channel {1}." -f $header.ClientId, $VC_NAME)
    Break
}

### File handling

# Check if the output file already exists
if (Test-Path -Path $OUTPUT_PATH) {
    # Ask the user if they want to overwrite the existing file
    $choice = $Host.UI.PromptForChoice("Overwrite existing file", "File at {0} already exists. Overwrite it?" -f $OUTPUT_PATH, @("&Yes", "&No"), 1)
    if ($choice -ne 0) {
        Write-Host("File transfer canceled by user.")
        Cleanup-Exit
    }
}

# Open the file (and recreate it if it already exists)
$fs = New-Object IO.FileStream($OUTPUT_PATH, [IO.FileMode]::Create)

### File transfer

Write-Host("Waiting for file transfer...")

# Read the file data and write chunks to file
[UInt64] $totalFileSize = 0
$isDone = $false;
while (!$isDone) {
    if (!(Read-Chunk $channel ([ref] $header) $payloadBuf ([ref] $payloadLen))) {
        Cleanup-Exit
    }

    # Handle chunk types
    switch ($header.ChunkType) {
        ID_START {
            Write-Error("Received unexpected START chunk for channel {1}." -f $VC_NAME)
            return $null
        }
        ID_DATA {
            # Write transferred data to file
            $fs.Write($payloadBuf, 0, $header.PayloadLen)
            $totalFileSize += $header.PayloadLen
            Break
        }
        ID_END {
            # We reached the end of the file
            $isDone = $true;
            Break
        }
        Default {
            Write-Error("Received chunk with unknown type {0} for channel {1}." -f $header.ChunkType, $VC_NAME)
            return $null
        }
    }
}

Write-Host("Wrote {0} bytes to file {1} for channel {2}." -f $totalFileSize, $OUTPUT_PATH, $VC_NAME)

### Cleanup

$fs.Close()
if (!(Close-VirtualChannel $channel)) {
    Write-Warning("Failed to close virtual channel, ignoring")
}

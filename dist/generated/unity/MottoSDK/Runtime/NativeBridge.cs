/*
 * MOTTO GENERATED CODE - DO NOT EDIT
 *
 * Protocol Version: 0xA1
 * Schema Fingerprint: a15f1919c06e581b
 * Generated At: 2026-02-01T06:43:23.447495157+00:00
 */

using System;
using System.Runtime.InteropServices;
using System.Text;

using System.Runtime.InteropServices;

namespace Motto.SDK
{
    /// <summary>Native bridge for DllImport or WebAssembly</summary>
    public static unsafe class NativeBridge
    {
        private const string DllName = "motto_native";

#if UNITY_WEBGL && !UNITY_EDITOR
        [DllImport("__Internal")]
        private static extern int motto_wasm_init(byte* wasmBytes, int wasmLength);

        [DllImport("__Internal")]
        private static extern int motto_wasm_encode(byte* input, int inputLength, byte* output, int outputCapacity);

        [DllImport("__Internal")]
        private static extern int motto_wasm_decode(byte* input, int inputLength, byte* output, int outputCapacity);
#else
        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_init();

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_encode(byte* input, int inputLength, byte* output, int outputCapacity);

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_decode(byte* input, int inputLength, byte* output, int outputCapacity);

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_compress(byte* input, int inputLength, byte* output, int outputCapacity, int level);

        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
        private static extern int motto_native_decompress(byte* input, int inputLength, byte* output, int outputCapacity);
#endif

        /// <summary>Initialize native library</summary>
        public static bool Initialize()
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            // WASM initialization handled by JavaScript
            return true;
#else
            return motto_native_init() == 0;
#endif
        }

        /// <summary>Encode using native implementation (zero-copy)</summary>
        public static int Encode(ReadOnlySpan<byte> input, Span<byte> output)
        {
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {
#if UNITY_WEBGL && !UNITY_EDITOR
                return motto_wasm_encode(inputPtr, input.Length, outputPtr, output.Length);
#else
                return motto_native_encode(inputPtr, input.Length, outputPtr, output.Length);
#endif
            }
        }

        /// <summary>Decode using native implementation (zero-copy)</summary>
        public static int Decode(ReadOnlySpan<byte> input, Span<byte> output)
        {
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {
#if UNITY_WEBGL && !UNITY_EDITOR
                return motto_wasm_decode(inputPtr, input.Length, outputPtr, output.Length);
#else
                return motto_native_decode(inputPtr, input.Length, outputPtr, output.Length);
#endif
            }
        }

#if !UNITY_WEBGL || UNITY_EDITOR
        /// <summary>Compress with Zstd (native only)</summary>
        public static int Compress(ReadOnlySpan<byte> input, Span<byte> output, int level = 3)
        {
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {
                return motto_native_compress(inputPtr, input.Length, outputPtr, output.Length, level);
            }
        }

        /// <summary>Decompress with Zstd (native only)</summary>
        public static int Decompress(ReadOnlySpan<byte> input, Span<byte> output)
        {
            fixed (byte* inputPtr = input)
            fixed (byte* outputPtr = output)
            {
                return motto_native_decompress(inputPtr, input.Length, outputPtr, output.Length);
            }
        }
#endif
    }
}

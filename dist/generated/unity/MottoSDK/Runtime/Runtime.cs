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

using System.Threading.Tasks;
using UnityEngine;

namespace Motto.SDK
{
    /// <summary>Connection state</summary>
    public enum ConnectionState
    {
        Disconnected,
        Connecting,
        Connected,
        Reconnecting,
        Error
    }

    /// <summary>Retry configuration</summary>
    [System.Serializable]
    public class RetryConfig
    {
        public int MaxRetries = 5;
        public int InitialDelayMs = 100;
        public int MaxDelayMs = 30000;
        public float BackoffMultiplier = 2.0f;
    }

    /// <summary>Calculate retry delay with exponential backoff</summary>
    public static class RetryHelper
    {
        public static int CalculateDelay(int attempt, RetryConfig config)
        {
            var delay = config.InitialDelayMs * Mathf.Pow(config.BackoffMultiplier, attempt);
            return Mathf.Min((int)delay, config.MaxDelayMs);
        }
    }

    /// <summary>Transport interface</summary>
    public interface IMottoTransport
    {
        ConnectionState State { get; }
        Task ConnectAsync();
        Task DisconnectAsync();
        Task SendAsync(byte[] data);
        event System.Action<byte[]> OnReceived;
    }

    /// <summary>WebSocket transport for Unity</summary>
    public class MottoWebSocketTransport : IMottoTransport
    {
        private readonly string _url;
        private readonly RetryConfig _retryConfig;
        private int _retryAttempt;

        public ConnectionState State { get; private set; } = ConnectionState.Disconnected;
        public event System.Action<byte[]> OnReceived;

        public MottoWebSocketTransport(string url, RetryConfig retryConfig = null)
        {
            _url = url;
            _retryConfig = retryConfig ?? new RetryConfig();
        }

        public async Task ConnectAsync()
        {
            State = ConnectionState.Connecting;

            try
            {
                // TODO: Implement actual WebSocket connection
                // For Unity, consider using NativeWebSocket or UnityWebSocket
                await Task.Delay(100); // Placeholder
                State = ConnectionState.Connected;
                _retryAttempt = 0;
            }
            catch (System.Exception)
            {
                State = ConnectionState.Error;
                throw;
            }
        }

        public async Task ReconnectAsync()
        {
            if (_retryAttempt >= _retryConfig.MaxRetries)
            {
                throw new System.Exception("Max retry attempts exceeded");
            }

            State = ConnectionState.Reconnecting;
            var delay = RetryHelper.CalculateDelay(_retryAttempt, _retryConfig);
            _retryAttempt++;

            await Task.Delay(delay);
            await ConnectAsync();
        }

        public async Task DisconnectAsync()
        {
            // TODO: Close WebSocket
            await Task.CompletedTask;
            State = ConnectionState.Disconnected;
        }

        public async Task SendAsync(byte[] data)
        {
            if (State != ConnectionState.Connected)
            {
                throw new System.InvalidOperationException("Not connected");
            }

            // TODO: Send via WebSocket
            await Task.CompletedTask;
        }
    }
}

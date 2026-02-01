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

namespace Motto.SDK
{
    /// <summary>Protocol version byte embedded in all packets</summary>
    public static class Protocol
    {
        public const byte VersionByte = 0xA1;
        public const string Fingerprint = "a15f1919c06e581b";
    }

    /// <summary>Zero-copy packet reader using Span</summary>
    public ref struct PacketReader
    {
        private ReadOnlySpan<byte> _data;
        private int _offset;

        public PacketReader(ReadOnlySpan<byte> data)
        {
            _data = data;
            _offset = 0;
        }

        public bool ValidateVersion()
        {
            return _data.Length > 0 && _data[0] == Protocol.VersionByte;
        }

        public void SkipVersionByte()
        {
            _offset = 1;
        }

        public byte ReadU8()
        {
            return _data[_offset++];
        }

        public ushort ReadU16()
        {
            var value = BitConverter.ToUInt16(_data.Slice(_offset, 2));
            _offset += 2;
            return value;
        }

        public uint ReadU32()
        {
            var value = BitConverter.ToUInt32(_data.Slice(_offset, 4));
            _offset += 4;
            return value;
        }

        public ulong ReadU64()
        {
            var value = BitConverter.ToUInt64(_data.Slice(_offset, 8));
            _offset += 8;
            return value;
        }

        public float ReadF32()
        {
            var value = BitConverter.ToSingle(_data.Slice(_offset, 4));
            _offset += 4;
            return value;
        }

        public double ReadF64()
        {
            var value = BitConverter.ToDouble(_data.Slice(_offset, 8));
            _offset += 8;
            return value;
        }

        public bool ReadBool()
        {
            return ReadU8() != 0;
        }

        public string ReadString()
        {
            var length = (int)ReadU32();
            var str = Encoding.UTF8.GetString(_data.Slice(_offset, length));
            _offset += length;
            return str;
        }

        public int Remaining => _data.Length - _offset;
    }

    /// <summary>Packet builder for encoding</summary>
    public class PacketBuilder
    {
        private byte[] _buffer;
        private int _offset;

        public PacketBuilder(int initialCapacity = 256)
        {
            _buffer = new byte[initialCapacity];
            // Write version byte header
            WriteU8(Protocol.VersionByte);
        }

        private void EnsureCapacity(int need)
        {
            if (_offset + need > _buffer.Length)
            {
                var newSize = Math.Max(_buffer.Length * 2, _offset + need);
                Array.Resize(ref _buffer, newSize);
            }
        }

        public void WriteU8(byte value)
        {
            EnsureCapacity(1);
            _buffer[_offset++] = value;
        }

        public void WriteU16(ushort value)
        {
            EnsureCapacity(2);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 2;
        }

        public void WriteU32(uint value)
        {
            EnsureCapacity(4);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 4;
        }

        public void WriteU64(ulong value)
        {
            EnsureCapacity(8);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 8;
        }

        public void WriteF32(float value)
        {
            EnsureCapacity(4);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 4;
        }

        public void WriteF64(double value)
        {
            EnsureCapacity(8);
            BitConverter.TryWriteBytes(_buffer.AsSpan(_offset), value);
            _offset += 8;
        }

        public void WriteBool(bool value)
        {
            WriteU8((byte)(value ? 1 : 0));
        }

        public void WriteString(string value)
        {
            var bytes = Encoding.UTF8.GetBytes(value);
            WriteU32((uint)bytes.Length);
            EnsureCapacity(bytes.Length);
            bytes.CopyTo(_buffer.AsSpan(_offset));
            _offset += bytes.Length;
        }

        public byte[] Build()
        {
            var result = new byte[_offset];
            Array.Copy(_buffer, result, _offset);
            return result;
        }
    }

    /// <summary>Codec extensions for Position</summary>
    public static class PositionCodec
    {
        public static byte[] Encode(this Position msg)
        {
            var builder = new PacketBuilder();
            builder.WriteF32(msg.X);
            builder.WriteF32(msg.Y);
            return builder.Build();
        }

        public static Position Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new Position
            {
                X = reader.ReadF32(),
                Y = reader.ReadF32(),
            };
        }
    }

    /// <summary>Codec extensions for Velocity</summary>
    public static class VelocityCodec
    {
        public static byte[] Encode(this Velocity msg)
        {
            var builder = new PacketBuilder();
            builder.WriteF32(msg.Dx);
            builder.WriteF32(msg.Dy);
            return builder.Build();
        }

        public static Velocity Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new Velocity
            {
                Dx = reader.ReadF32(),
                Dy = reader.ReadF32(),
            };
        }
    }

    /// <summary>Codec extensions for Player</summary>
    public static class PlayerCodec
    {
        public static byte[] Encode(this Player msg)
        {
            var builder = new PacketBuilder();
            /* TODO: encode PlayerId */;
            builder.WriteString(msg.Name);
            /* TODO: encode Position */;
            /* TODO: encode Velocity */;
            builder.WriteU8(msg.Health);
            builder.WriteU32(msg.Score);
            /* TODO: encode PlayerStatus */;
            if (msg.AvatarUrl != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<String> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            return builder.Build();
        }

        public static Player Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new Player
            {
                Id = default /* TODO: decode PlayerId */,
                Name = reader.ReadString(),
                Position = default /* TODO: decode Position */,
                Velocity = default /* TODO: decode Velocity */,
                Health = reader.ReadU8(),
                Score = reader.ReadU32(),
                Status = default /* TODO: decode PlayerStatus */,
                AvatarUrl = reader.ReadU8() != 0 ? default /* TODO: decode Option<String> */ : default,
            };
        }
    }

    /// <summary>Codec extensions for RoomConfig</summary>
    public static class RoomConfigCodec
    {
        public static byte[] Encode(this RoomConfig msg)
        {
            var builder = new PacketBuilder();
            /* TODO: encode RoomId */;
            builder.WriteString(msg.Name);
            builder.WriteU8(msg.MaxPlayers);
            builder.WriteBool(msg.IsPublic);
            builder.WriteString(msg.GameMode);
            if (msg.Settings != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<String> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            return builder.Build();
        }

        public static RoomConfig Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new RoomConfig
            {
                Id = default /* TODO: decode RoomId */,
                Name = reader.ReadString(),
                MaxPlayers = reader.ReadU8(),
                IsPublic = reader.ReadBool(),
                GameMode = reader.ReadString(),
                Settings = reader.ReadU8() != 0 ? default /* TODO: decode Option<String> */ : default,
            };
        }
    }

    /// <summary>Codec extensions for GameState</summary>
    public static class GameStateCodec
    {
        public static byte[] Encode(this GameState msg)
        {
            var builder = new PacketBuilder();
            builder.WriteU64(msg.Tick);
            builder.WriteU64(msg.Timestamp);
            /* TODO: encode Vec<Player> */;
            /* TODO: encode RoomConfig */;
            builder.WriteBool(msg.Paused);
            return builder.Build();
        }

        public static GameState Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new GameState
            {
                Tick = reader.ReadU64(),
                Timestamp = reader.ReadU64(),
                Players = default /* TODO: decode Vec<Player> */,
                Room = default /* TODO: decode RoomConfig */,
                Paused = reader.ReadBool(),
            };
        }
    }

    /// <summary>Codec extensions for PlayerUpdate</summary>
    public static class PlayerUpdateCodec
    {
        public static byte[] Encode(this PlayerUpdate msg)
        {
            var builder = new PacketBuilder();
            /* TODO: encode PlayerId */;
            if (msg.Position != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<Position> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            if (msg.Velocity != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<Velocity> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            if (msg.Health != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<u8> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            if (msg.Score != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<u32> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            return builder.Build();
        }

        public static PlayerUpdate Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new PlayerUpdate
            {
                Id = default /* TODO: decode PlayerId */,
                Position = reader.ReadU8() != 0 ? default /* TODO: decode Option<Position> */ : default,
                Velocity = reader.ReadU8() != 0 ? default /* TODO: decode Option<Velocity> */ : default,
                Health = reader.ReadU8() != 0 ? default /* TODO: decode Option<u8> */ : default,
                Score = reader.ReadU8() != 0 ? default /* TODO: decode Option<u32> */ : default,
            };
        }
    }

    /// <summary>Codec extensions for Paginated</summary>
    public static class PaginatedCodec
    {
        public static byte[] Encode(this Paginated msg)
        {
            var builder = new PacketBuilder();
            /* TODO: encode Vec<T> */;
            builder.WriteU32(msg.Total);
            builder.WriteU32(msg.Page);
            builder.WriteU32(msg.PerPage);
            return builder.Build();
        }

        public static Paginated Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new Paginated
            {
                Items = default /* TODO: decode Vec<T> */,
                Total = reader.ReadU32(),
                Page = reader.ReadU32(),
                PerPage = reader.ReadU32(),
            };
        }
    }

    /// <summary>Codec extensions for ApiResult</summary>
    public static class ApiResultCodec
    {
        public static byte[] Encode(this ApiResult msg)
        {
            var builder = new PacketBuilder();
            builder.WriteBool(msg.Success);
            if (msg.Data != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<T> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            if (msg.Error != null)
            {
                builder.WriteU8(1);
                /* TODO: encode Option<String> */;
            }
            else
            {
                builder.WriteU8(0);
            }
            builder.WriteString(msg.RequestId);
            return builder.Build();
        }

        public static ApiResult Decode(ReadOnlySpan<byte> data)
        {
            var reader = new PacketReader(data);
            if (!reader.ValidateVersion()) return default;
            reader.SkipVersionByte();

            return new ApiResult
            {
                Success = reader.ReadBool(),
                Data = reader.ReadU8() != 0 ? default /* TODO: decode Option<T> */ : default,
                Error = reader.ReadU8() != 0 ? default /* TODO: decode Option<String> */ : default,
                RequestId = reader.ReadString(),
            };
        }
    }
}

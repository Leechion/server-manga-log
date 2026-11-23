import struct, zlib, sys

def read_p6(path):
    data = open(path, 'rb').read()
    # header: P6\n W H\n 255\n
    if data[:2] != b'P6':
        raise ValueError('not P6')
    # tokenize skipping comments/whitespace
    i = 2
    vals = []
    while len(vals) < 3:
        # skip whitespace
        while i < len(data) and data[i:i+1].isspace():
            i += 1
        # skip comment
        if data[i:i+1] == b'#':
            while i < len(data) and data[i:i+1] != b'\n':
                i += 1
            continue
        j = i
        while j < len(data) and not data[j:j+1].isspace():
            j += 1
        vals.append(int(data[i:j]))
        i = j
    w, h, maxv = vals
    assert maxv == 255
    i += 1  # single whitespace after maxv
    rgb = data[i:i + w*h*3]
    assert len(rgb) == w*h*3, (len(rgb), w*h*3)
    return w, h, rgb

def write_png(path, w, h, rgb):
    def chunk(typ, payload):
        c = struct.pack('>I', len(payload)) + typ + payload
        c += struct.pack('>I', zlib.crc32(typ + payload) & 0xffffffff)
        return c
    sig = b'\x89PNG\r\n\x1a\n'
    ihdr = struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)
    raw = bytearray()
    stride = w*3
    for y in range(h):
        raw.append(0)  # filter type 0
        raw += rgb[y*stride:(y+1)*stride]
    idat = zlib.compress(bytes(raw), 6)
    png = sig + chunk(b'IHDR', ihdr) + chunk(b'IDAT', idat) + chunk(b'IEND', b'')
    open(path, 'wb').write(png)

if __name__ == '__main__':
    src, dst = sys.argv[1], sys.argv[2]
    w, h, rgb = read_p6(src)
    write_png(dst, w, h, rgb)
    print(f'{src} -> {dst}  {w}x{h}')

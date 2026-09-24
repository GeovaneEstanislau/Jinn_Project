import binascii
b=open('limine/usr/bin/limine','rb').read(20)
print(binascii.hexlify(b).decode())
print(list(b))

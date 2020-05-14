import socket, time

current_milli_time = lambda: time.time_ns()

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.connect(('localhost', 6666))
s.sendall(b'{"id": "94db283e-4263-4215-5262-61bbb4e54d2b", "name": "sil"}\n')
data = s.recv(1024)
start = current_milli_time()
s.sendall(b'hallo bitch\n')

data = s.recv(1024)
data = s.recv(1024)
data = s.recv(1024)
# data = s.recv(1024)

end = current_milli_time()
s.close()
print('Received', repr(data))
print(end - start)

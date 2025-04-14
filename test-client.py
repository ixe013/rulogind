import socket
import sys
import os
import select

def rlogin_client(host, port, login_user, server_user=None, term_type='dumb'):
    if server_user is None:
        server_user = login_user

    try:
        client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        client_socket.connect((host, port))
        print(f"Connecté à {host}:{port}")

        # Envoi des informations d'identification (null-terminated strings)
        credentials = f"\0{login_user}\0{server_user}\0{term_type}\0".encode('utf-8')
        client_socket.sendall(credentials)

        # Interaction avec le shell
        while True:
            read_sockets, _, _ = select.select([sys.stdin, client_socket], [], [])
            for sock in read_sockets:
                if sock == client_socket:
                    data = client_socket.recv(4096)
                    if not data:
                        print("\nConnexion fermée par le serveur.")
                        return
                    sys.stdout.buffer.write(data)
                    sys.stdout.flush()
                else:
                    message = sys.stdin.readline().encode('utf-8')
                    client_socket.sendall(message)

    except ConnectionRefusedError:
      print(f"Erreur : Connexion refusée à {host}:{port}. Assurez-vous que le serveur rlogin est en cours d'exécution.")
    except socket.gaierror:
        print(f"Erreur : Impossible de résoudre l'hôte '{host}'.")
    except Exception as e:
        print(f"Une erreur s'est produite : {e}")
    finally:
        if 'client_socket' in locals() and client_socket:
            client_socket.close()

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: python rlogin_client.py <host> <port> <login_user>")
        sys.exit(1)

    host = sys.argv[1]
    port = int(sys.argv[2])
    login_user = sys.argv[3]
    rlogin_client(host, port, login_user)

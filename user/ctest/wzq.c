#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <sys/socket.h>

#define SIZE     15
#define PORT     8080
#define SERVERIP "10.0.0.1"
#define CLIENTIP "10.0.0.2"
#define EMPTY  0
#define BLACK  1
#define WHITE  2

static int board[SIZE][SIZE];
static int cursor_x, cursor_y;
static int current_player;
static int my_color;
static int sock;
static struct sockaddr_in peer_addr;

static void draw_board(void) {
    printf("\033[2J\033[H");
    printf("  === Gomoku ===\n");
    printf("  You: %s  Peer: %s\n\n",
           my_color == BLACK ? "O" : "X",
           my_color == BLACK ? "X" : "O");

    for (int row = SIZE - 1; row >= 0; row--) {
        printf(" %2d ", row + 1);
        for (int col = 0; col < SIZE; col++) {
            if (col == cursor_x && row == cursor_y) {
                if (board[row][col] == BLACK)      printf("[O]");
                else if (board[row][col] == WHITE)  printf("[X]");
                else                                 printf("[+]");
            } else {
                if (board[row][col] == BLACK)      printf(" O ");
                else if (board[row][col] == WHITE)  printf(" X ");
                else                                 printf(" . ");
            }
        }
        printf("\n");
    }

    printf("    ");
    for (int col = 0; col < SIZE; col++)
        printf("%2d ", col + 1);
    printf("\n\n");

    if (current_player == my_color)
        printf("  Your turn (%s)\n", my_color == BLACK ? "O" : "X");
    else
        printf("  Waiting...\n");

    printf("  Arrow=move  Enter=place  q=quit\n> ");
    fflush(stdout);
}

static int check_line(int x, int y, int dx, int dy, int player) {
    for (int i = 0; i < 5; i++) {
        int nx = x + i * dx, ny = y + i * dy;
        if (nx < 0 || nx >= SIZE || ny < 0 || ny >= SIZE) return 0;
        if (board[ny][nx] != player) return 0;
    }
    return 1;
}

static int check_win(int x, int y, int player) {
    int dirs[4][2] = {{1,0},{0,1},{1,1},{1,-1}};
    for (int d = 0; d < 4; d++) {
        for (int s = -4; s <= 0; s++) {
            if (check_line(x + s*dirs[d][0], y + s*dirs[d][1],
                           dirs[d][0], dirs[d][1], player))
                return 1;
        }
    }
    return 0;
}

static int board_full(void) {
    for (int r = 0; r < SIZE; r++)
        for (int c = 0; c < SIZE; c++)
            if (board[r][c] == EMPTY) return 0;
    return 1;
}

static void send_move(int x, int y) {
    char msg[2];
    msg[0] = (char)x;
    msg[1] = (char)y;
    sendto(sock, msg, 2, 0,
           (struct sockaddr *)&peer_addr, sizeof(peer_addr));
}

static int recv_move(int *ox, int *oy) {
    char buf[4];
    struct sockaddr_in from;
    socklen_t len = sizeof(from);
    int n = recvfrom(sock, buf, sizeof(buf), 0,
                     (struct sockaddr *)&from, &len);
    if (n < 2) return -1;
    *ox = (unsigned char)buf[0];
    *oy = (unsigned char)buf[1];
    if (*ox >= SIZE || *oy >= SIZE) return -1;
    if (board[*oy][*ox] != EMPTY) return -1;
    return 0;
}

static int setup_server(void) {
    sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock < 0) { perror("socket"); exit(1); }

    struct sockaddr_in addr = {0};
    addr.sin_family      = AF_INET;
    addr.sin_port        = htons(PORT);
    addr.sin_addr.s_addr = inet_addr(SERVERIP);

    if (bind(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        perror("bind"); exit(1);
    }

    printf("[Server] Listening on %s:%d, waiting for client...\n", SERVERIP, PORT);

    char buf[4];
    socklen_t len = sizeof(peer_addr);
    recvfrom(sock, buf, sizeof(buf), 0,
             (struct sockaddr *)&peer_addr, &len);

    printf("[Server] Client connected from %s:%d\n",
           inet_ntoa(peer_addr.sin_addr), ntohs(peer_addr.sin_port));
    printf("[Server] You are O (first)\n\n");
    return 1;
}

static int setup_client(void) {
    sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock < 0) { perror("socket"); exit(1); }

    /* Bind to my own IP */
    struct sockaddr_in my_addr = {0};
    my_addr.sin_family      = AF_INET;
    my_addr.sin_port        = htons(0); /* OS picks port */
    my_addr.sin_addr.s_addr = inet_addr(CLIENTIP);

    if (bind(sock, (struct sockaddr *)&my_addr, sizeof(my_addr)) < 0) {
        perror("bind"); exit(1);
    }

    /* Set peer = server */
    peer_addr.sin_family      = AF_INET;
    peer_addr.sin_port        = htons(PORT);
    peer_addr.sin_addr.s_addr = inet_addr(SERVERIP);

    char hello[] = "hi";
    sendto(sock, hello, 2, 0,
           (struct sockaddr *)&peer_addr, sizeof(peer_addr));

    printf("[Client] My IP: %s\n", CLIENTIP);
    printf("[Client] Connected to server %s:%d\n", SERVERIP, PORT);
    printf("[Client] You are X (second)\n\n");
    return 2;
}

/* My turn: block on stdin, parse arrow keys + enter */
static int my_turn(void) {
    draw_board();
    while (1) {
        char c;
        if (read(STDIN_FILENO, &c, 1) != 1) continue;

        if (c == 'q' || c == 'Q') return -1;

        if (c == '\033') {
            char seq[2];
            if (read(STDIN_FILENO, &seq[0], 1) != 1) continue;
            if (read(STDIN_FILENO, &seq[1], 1) != 1) continue;
            if (seq[0] == '[') {
                switch (seq[1]) {
                case 'A': if (cursor_y < SIZE-1) cursor_y++; break;
                case 'B': if (cursor_y > 0)      cursor_y--; break;
                case 'C': if (cursor_x < SIZE-1) cursor_x++; break;
                case 'D': if (cursor_x > 0)      cursor_x--; break;
                }
            }
            draw_board();
        } else if (c == '\n' || c == '\r') {
            if (board[cursor_y][cursor_x] != EMPTY) {
                draw_board();
                printf("  Already occupied\n> ");
                fflush(stdout);
                continue;
            }
            board[cursor_y][cursor_x] = my_color;
            send_move(cursor_x, cursor_y);
            return 0;
        }
    }
}

/* Opponent's turn: block on recvfrom */
static int opp_turn(void) {
    draw_board();
    int ox, oy;
    while (recv_move(&ox, &oy) < 0)
        ; /* retry on bad packet */

    int opp = (my_color == BLACK) ? WHITE : BLACK;
    board[oy][ox] = opp;
    return check_win(ox, oy, opp) ? 1 : 0;
}

int main(void) {
    int choice;
    printf("=== Gomoku ===\n");
    printf("1. Server (listen on %s:%d)\n", SERVERIP, PORT);
    printf("2. Client (connect to %s:%d, my IP: %s)\n", SERVERIP, PORT, CLIENTIP);
    printf("Choice: ");
    scanf("%d", &choice);
    /* Flush leftover newline from scanf */
    { int c; while ((c = getchar()) != '\n' && c != EOF); }

    if (choice == 1)
        my_color = setup_server();
    else if (choice == 2)
        my_color = setup_client();
    else {
        printf("Bad choice\n");
        return 1;
    }

    memset(board, 0, sizeof(board));
    cursor_x = SIZE / 2;
    cursor_y = SIZE / 2;
    current_player = BLACK;

    while (1) {
        if (current_player == my_color) {
            /* My turn: read keyboard */
            int ret = my_turn();
            if (ret < 0) break; /* quit */
            if (check_win(cursor_x, cursor_y, my_color)) {
                draw_board();
                printf("\n  *** YOU WIN! ***\n\n");
                break;
            }
            if (board_full()) {
                draw_board();
                printf("\n  *** DRAW ***\n\n");
                break;
            }
            current_player = (my_color == BLACK) ? WHITE : BLACK;
        } else {
            /* Opponent's turn: block on network */
            int ret = opp_turn();
            if (ret > 0) {
                draw_board();
                printf("\n  *** OPPONENT WINS ***\n\n");
                break;
            }
            if (board_full()) {
                draw_board();
                printf("\n  *** DRAW ***\n\n");
                break;
            }
            current_player = my_color;
        }
    }

    close(sock);
    printf("Bye!\n");
    return 0;
}

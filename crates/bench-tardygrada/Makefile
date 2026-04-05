CC = cc
CFLAGS = -std=c11 -O2 -Wall -Wextra
TARDY = tardygrada

# Find all tardygrada .c files except main.c
TARDY_SRC = $(filter-out $(TARDY)/main.c, $(wildcard $(TARDY)/vm/*.c $(TARDY)/coordinate/*.c $(TARDY)/compiler/*.c $(TARDY)/ontology/*.c $(TARDY)/verify/*.c $(TARDY)/mcp/*.c))

# Monocypher needs different flags
MONO_SRC = $(wildcard $(TARDY)/vm/monocypher/*.c)
TARDY_SRC_NO_MONO = $(filter-out $(MONO_SRC), $(TARDY_SRC))

all: bench_pure

bench_pure: bench.c $(TARDY_SRC_NO_MONO) $(MONO_SRC)
	$(CC) $(CFLAGS) -I$(TARDY) -pedantic $(TARDY_SRC_NO_MONO) bench.c -o $@.tmp.o -c
	$(CC) -std=c11 -O2 -Wall -I$(TARDY) $(MONO_SRC) -c -o mono.tmp.o
	# Actually, let's just compile everything together
	$(CC) $(CFLAGS) -I$(TARDY) bench.c $(TARDY_SRC_NO_MONO) -o bench_pure
	@# Monocypher needs to be compiled separately if pedantic causes issues

# Simpler approach: compile everything without pedantic
bench_pure_simple: bench.c
	$(CC) -std=c11 -O2 -Wall -I$(TARDY) bench.c $(TARDY_SRC) -o bench_pure

run: bench_pure_simple
	./bench_pure

size: bench_pure_simple
	@ls -lh bench_pure
	@size bench_pure 2>/dev/null || true

clean:
	rm -f bench_pure *.tmp.o mono.tmp.o

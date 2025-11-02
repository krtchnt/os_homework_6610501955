CXX ?= g++
CXXFLAGS ?= -O3 -std=c++20 -Wall -Wextra -pedantic -fopenmp
LDFLAGS ?= -fopenmp

TARGET := parallel
SRC := parallel.cpp

all: $(TARGET)

$(TARGET): $(SRC)
	$(CXX) $(CXXFLAGS) -o $@ $< $(LDFLAGS)

.PHONY: clean
clean:
	$(RM) $(TARGET)

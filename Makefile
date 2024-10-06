CXX = "/mnt/c//Program Files/msys64/mingw64/bin/g++.exe"
WINDRES = "/mnt/c/Program Files/msys64/mingw64/bin/windres.exe"
CXXFLAGS = -mwindows -Iinclude
PROJECT_NAME = borde
all: $(PROJECT_NAME).exe

$(PROJECT_NAME).exe: src/main.cpp res/resource.o
	$(CXX) $(CXXFLAGS) -o $(PROJECT_NAME).exe src/main.cpp res/resource.o

res/resource.o: res/resource.rc
	$(WINDRES) res/resource.rc -o res/resource.o
	
clean:
	rm -f $(PROJECT_NAME).exe res/resource.o
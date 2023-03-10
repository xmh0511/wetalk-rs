#include <iostream>
#include <string>
#include <sys/ipc.h>
#include <sys/shm.h>
#include <cstring>
int main(){
	auto r = shmget((key_t)1234, 1024, 0666|IPC_CREAT);
	std::cout<< r<<std::endl;
	auto addr = shmat(r,0,0);
	std::cout<< addr<<std::endl;
	char* ptr = (char*)addr;
	std::cout<<"at index 12: "<< (int)ptr[12]<<std::endl;
	auto&& arr = "hello, world";
	std::cout<<"total string: "<< sizeof(arr)<<std::endl;
	memcpy(ptr,arr, sizeof(arr));
	std::cout<< ptr<<std::endl;
	std::cin.get();
}
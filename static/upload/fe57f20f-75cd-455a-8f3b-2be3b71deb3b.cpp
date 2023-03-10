#include <atomic>
#include <thread>
#include <cassert>

int main(){
	for(int i = 0; i<1000000;i++){
		std::atomic<bool> x = false,y = false;
		std::atomic<int> z = 0;
		auto write_x_then_y = std::thread([&](){
			x.store(true,std::memory_order_relaxed);
			y.store(true,std::memory_order_release);
		});
		auto read_y_then_x = std::thread([&](){
			while(!y.load(std::memory_order_consume)); //consume
			if(x.load(std::memory_order_relaxed)){
				++z;
			}
		});
		write_x_then_y.join();
		read_y_then_x.join();
		auto r = z.load();
		assert(r!=0);
	}
}
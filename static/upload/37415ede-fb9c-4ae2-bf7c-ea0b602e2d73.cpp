#include <iostream>
#include <memory>

template<class T>
struct ResourceManager{
    T* ptr = nullptr;
    int strong_counter_ = 0;
    int weak_counter_ = 0;
    ~ResourceManager(){
        std::cout<<"destroy ResourceManager\n";
    }
};
template<class T>
struct WeakPtr{
    mutable ResourceManager<T>* manager = nullptr;
    WeakPtr(){}
    WeakPtr(ResourceManager<T>* t):manager(t){
		std::cout<<"weakptr from manager\n";
        manager->weak_counter_++;
    }
    WeakPtr(WeakPtr const& r):manager(r.manager){
        manager->weak_counter_++;
    }
    ~WeakPtr(){
		std::cout<<"destroy WeakPtr\n";
		if(manager){
			//std::cout<< manager->weak_counter_<<std::endl;
			manager->weak_counter_--;
			if(manager->weak_counter_ == 0){
				delete manager;
			}
		}
    }
    WeakPtr& operator=(WeakPtr const& r){
		r.manager->weak_counter_++;
        if(manager){
            manager->weak_counter_--;
            if(manager->weak_counter_ == 0){
                delete manager;
            }
        }
        manager = r.manager;
        return *this;
    }
};
template<class T>
struct Arc{
    mutable ResourceManager<T>* manager = nullptr;
	Arc() = default;
    Arc(T* ptr):manager(new ResourceManager<T>{ptr,1,0}){}
    ~Arc(){
        std::cout<<"destroy Arc\n";
		if(manager){
			manager->strong_counter_--;
			std::cout<< manager->strong_counter_<<std::endl;
			if(manager->strong_counter_ == 0){
				delete manager->ptr;
				manager->ptr = nullptr;
				std::cout<< "manager->weak_counter_"<< manager->weak_counter_<<"\n";
				if(manager->weak_counter_ == 0){
						delete manager;
				}
			}
		}
    }
    Arc(Arc const& p):manager(p.manager){
       (p.manager)->strong_counter_++;
    }
    WeakPtr<T> downgrade(){
        return WeakPtr<T>{manager};
    }
	Arc& operator=(Arc const& r){
		r.manager->strong_counter_++;
		if(manager){
			manager->strong_counter_--;
			if(manager->strong_counter_ == 0){
				delete manager->ptr;
				if(manager->weak_counter_ == 0){
					delete manager;
				}
			}
		}
		manager = r.manager;
		return *this;
	}
};
struct Test{
    Test(){}
    ~Test(){
        std::cout<<"destroy Test\n";
    }
};
int main(){
    // WeakPtr<Test> weak;
	// {
	// 	Arc<Test> arc;
	// 	{
	// 		auto smart = Arc<Test>(new Test);
	// 		arc = smart;
	// 		weak = smart.downgrade();
	// 	}
	// 	std::cout<<"exit 96\n";
	// }
	// std::cout<<"prepare to exit\n";

	Arc<Test> arc_g;
	{
		WeakPtr<Test> weak;
		Arc<Test> arc;
		{
			auto smart = Arc<Test>(new Test);
			arc = smart;
			weak = smart.downgrade();
			weak = weak;
			arc_g = smart;
		}
	}
	std::cout<<"prepare to exit\n";
	std::shared_ptr<int>
}
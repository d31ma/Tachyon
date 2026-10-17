namespace tachyon {

class TacNeeds {
public:
    template <typename Provider>
    static Provider& of() {
        static Provider provider;
        return provider;
    }

    template <typename Provider>
    static std::shared_ptr<Provider> fresh() {
        return std::make_shared<Provider>();
    }
};

}

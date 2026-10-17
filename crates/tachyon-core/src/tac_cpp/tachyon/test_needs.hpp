namespace tachyon {

template <typename Provider>
class TacFake {
public:
    explicit TacFake(Provider& fake) : previous(slot()) {
        slot() = &fake;
    }

    ~TacFake() {
        slot() = previous;
    }

    TacFake(const TacFake&) = delete;
    TacFake& operator=(const TacFake&) = delete;

    static Provider*& slot() {
        static Provider* current = nullptr;
        return current;
    }

private:
    Provider* previous;
};

class TacSharedMaking {
public:
    TacSharedMaking() : previous(active()) {
        active() = true;
    }

    ~TacSharedMaking() {
        active() = previous;
    }

    TacSharedMaking(const TacSharedMaking&) = delete;
    TacSharedMaking& operator=(const TacSharedMaking&) = delete;

    static bool& active() {
        static bool making = false;
        return making;
    }

private:
    bool previous;
};

class TacFreshStart {
public:
    static void next() {
        generation() += 1;
    }

    static unsigned& generation() {
        static unsigned current = 0;
        return current;
    }
};

class TacNeeds {
public:
    template <typename Provider>
    static Provider& of() {
        Provider* fake = TacFake<Provider>::slot();
        if (fake != nullptr && !TacSharedMaking::active()) {
            return *fake;
        }
        TacSharedMaking making;
        static std::vector<std::shared_ptr<Provider>> made;
        static unsigned madeIn = 0;
        if (made.empty() || madeIn != TacFreshStart::generation()) {
            madeIn = TacFreshStart::generation();
            made.push_back(std::make_shared<Provider>());
        }
        return *made.back();
    }

    template <typename Provider>
    static std::shared_ptr<Provider> fresh() {
        Provider* fake = TacFake<Provider>::slot();
        if (fake != nullptr && !TacSharedMaking::active()) {
            return std::shared_ptr<Provider>(fake, [](Provider*) {});
        }
        return std::make_shared<Provider>();
    }
};

}

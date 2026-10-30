// 05 类与对象
// class 里写字段和方法，init 是构造方法，self 指向当前实例

class Animal
    str name = ""
    int hp = 10

    function init(name, hp)
        self.name = name
        self.hp = hp

    function speak()
        return "……"

    function hurt(damage)
        hp -= damage
        if (hp < 0)
            hp = 0
        return hp

class Dog extends Animal
    str breed = "土狗"

    function init(name, breed)
        super(name, 20)
        self.breed = breed

    // 覆盖父类方法，方法里可以直接用 self.name
    function speak()
        return join(name, "（", breed, "）：汪！")

class Cat extends Animal
    function speak()
        return join(name, "：喵～")

Dog d = new Dog("大黄", "中华田园犬")
print(d.speak())
print(join("血量 ", d.hp))

d.hurt(5)
print(join("受伤后 ", d.hp))

Cat c = new Cat("咪咪", 8)
print(c.speak())

// 对象可以放进列表，再逐个调用方法
list pets
add d to pets
add c to pets
for p in pets
    print(p.speak())
    print(join("  剩余血量：", p.hp, "，类型：", type(p)))

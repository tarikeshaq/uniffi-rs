import uniffi.sprites.*;

val s = Sprite(Point(0.0,1.0))
println("Sprite ${s} is at position ${s.getPosition()}")
s.moveTo(Point(1.0, 2.0))
println("Sprite ${s} has moved to position ${s.getPosition()}")
s.moveBy(Vector(-4.0, 2.0))
println("Sprite ${s} has moved to position ${s.getPosition()}")


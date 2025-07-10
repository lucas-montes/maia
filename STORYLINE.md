I always wanted to create my own assistant.

I tried vibecoind it. It worked right? i havent checked the code yet, so let's see.
it made it work, but the code is awfull. a lot of duplication, useless complexity, in resume a good old spagetthi
Another shitty things is the use of clone. instead of using references or passing the values it cloned everything

```rust
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }
```


It didn't reuse existing code. Maybe is on my fault, but it ignored some instructions that was given to him.
It did whatever the fuck it wanted.

I didn't like the experience, the code is awfull, I don't even want to touch it. Everything is good to trowh away.

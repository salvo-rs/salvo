use crate::Context;

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, ctx: &mut Context);
}

impl<F> Handler for F where F: Send + Sync + 'static + Fn(&mut Context) {
  fn handle(&self, ctx: &mut Context){
    (*self)(ctx);
  }
}

impl<H> Handler for [H] where H: Handler {
  fn handle(&self, ctx: &mut Context){
    for h in self {
      if !ctx.is_commited() {
        h.handle(ctx);
      } else {
        break;
      }
    }
  }
}
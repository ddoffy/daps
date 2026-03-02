use clipboard::{ClipboardContext, ClipboardProvider};

pub struct Cpboard<'a> {
    ctx: &'a mut ClipboardContext,
}

impl<'a> Cpboard<'a> {
    pub fn new(ctx: &'a mut ClipboardContext) -> Cpboard<'a> {
        Cpboard { ctx }
    }

    pub fn set_clipboard_content(
        &mut self,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.ctx.set_contents(content.to_owned())?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_clipboard_content(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        self.ctx.get_contents()
    }
}

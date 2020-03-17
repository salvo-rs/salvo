use std::fs::File;
use std::io::prelude::*;
use async_trait::async_trait;

#[async_trait]
pub trait BodyWriter: Send {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()>;
}

#[async_trait]
impl BodyWriter for String {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        dest.write_all(self.as_bytes())
    }
}

#[async_trait]
impl<'a> BodyWriter for &'a str {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        dest.write_all(self.as_bytes())
    }
}

#[async_trait]
impl BodyWriter for Vec<u8> {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        dest.write_all(self)
    }
}

#[async_trait]
impl<'a> BodyWriter for &'a [u8] {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        dest.write_all(self)
    }
}

#[async_trait]
impl BodyWriter for File {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        std::io::copy(self, dest).map(|_| ())
    }
}

#[async_trait]
impl BodyWriter for Box<dyn std::io::Read + Send> {
    async fn write(&mut self, dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        std::io::copy(self, dest).map(|_| ())
    }
}

#[async_trait]
impl BodyWriter for () {
    async fn write(&mut self, _dest: &mut (dyn Write + Send)) -> std::io::Result<()> {
        Ok(())
    }
}

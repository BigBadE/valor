use anyhow::Error;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use url::Url;

pub struct PageState {
    state: State
}

pub struct PageData {

}

pub enum State {
    Loading(JoinHandle<PageData>),
    Loaded(PageData),
}

impl PageState {
    pub fn new(handle: &Handle, url: Url) -> Self {
        Self {
            state: State::Loading(handle.spawn(PageState::load(url)))
        }
    }

    pub async fn update(&mut self) -> Result<&PageData, Error> {
        if let State::Loading(handle) = &mut self.state {
            self.state = State::Loaded(handle.await?);
        }

        if let State::Loaded(data) = &self.state {
            Ok(data)
        } else {
            unreachable!()
        }
    }

    async fn load(url: Url) -> PageData {
        // Load the page data from the URL and parse it

    }
}

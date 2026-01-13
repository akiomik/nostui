use futures::stream;
use futures::stream::{BoxStream, StreamExt};
use nowhear::{MediaEvent, MediaSource, MediaSourceBuilder, MediaSourceError};
use tears::{SubscriptionId, SubscriptionSource};

#[derive(Clone, Debug, Default)]
pub struct MediaEvents;

impl MediaEvents {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl SubscriptionSource for MediaEvents {
    type Output = Result<MediaEvent, MediaSourceError>;

    fn stream(&self) -> BoxStream<'static, Self::Output> {
        stream::once(async {
            let source = MediaSourceBuilder::new().build().await?;
            source.event_stream().await
        })
        .flat_map(
            |result: Result<BoxStream<'static, MediaEvent>, MediaSourceError>| match result {
                Ok(stream) => stream.map(Ok).boxed(),
                Err(e) => stream::once(async move { Err(e) }).boxed(),
            },
        )
        .boxed()
    }

    fn id(&self) -> SubscriptionId {
        SubscriptionId::of::<Self>(42)
    }
}

#[cfg(test)]
mod tests {}

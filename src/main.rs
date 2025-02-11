use tokenizer::tokenizer_server::{Tokenizer, TokenizerServer};
use tokenizer::{TokenizeRequest, TokenizeResponse};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status};

use std::io::Read;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use ruzstd::decoding::StreamingDecoder;

use vibrato::{tokenizer::worker::Worker, Dictionary, Tokenizer as VibratoTokenizer};

type TokenizeResult<T> = Result<Response<T>, Status>;
type ResponseStream = Pin<Box<dyn Stream<Item = Result<TokenizeResponse, Status>> + Send>>;

pub mod tokenizer {
    tonic::include_proto!("tokenizer");
}

pub struct NyarsTokenizer {
    tokenize_worker: Arc<Mutex<Worker<'static>>>,
}

#[tonic::async_trait]
impl Tokenizer for NyarsTokenizer {
    type TokenizeStream = ResponseStream;

    async fn tokenize(
        &self,
        request: Request<TokenizeRequest>,
    ) -> TokenizeResult<Self::TokenizeStream> {
        let text = request.into_inner().request;

        let mut worker = self.tokenize_worker.lock().unwrap();
        worker.reset_sentence(&text);
        worker.tokenize();

        // Collect tokens into a Vec before streaming
        let tokens: Vec<TokenizeResponse> = worker
            .token_iter()
            .map(|token| TokenizeResponse {
                surface: token.surface().to_string(),
                feature: token.feature().to_string(),
            })
            .collect();

        let mut stream = Box::pin(tokio_stream::iter(tokens));

        // spawn and channel are required if you want handle "disconnect" functionality
        // the `out_stream` will not be polled after client disconnect
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                if tx.send(Ok(item)).await.is_err() {
                    // Client disconnected
                    break;
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::TokenizeStream
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse()?;

    let model_data = include_bytes!("../system.dic.zst");
    let mut decoder = StreamingDecoder::new(model_data.as_slice()).unwrap();
    let mut buff = vec![];
    decoder.read_to_end(&mut buff).unwrap();
    let dict = Dictionary::read(buff.as_slice())
        .unwrap()
        .reset_user_lexicon_from_reader(Some(include_str!("../parser.csv").as_bytes()))
        .unwrap();

    // Wrap Tokenizer in Arc to ensure shared ownership
    let tokenizer = Arc::new(VibratoTokenizer::new(dict));

    // Create Worker with a static reference
    let worker = unsafe { std::mem::transmute::<Worker, Worker<'static>>(tokenizer.new_worker()) };

    let tokenizer_service = NyarsTokenizer {
        tokenize_worker: Arc::new(Mutex::new(worker)),
    };

    println!("Starting gRPC server on {}", addr);
    Server::builder()
        .add_service(TokenizerServer::new(tokenizer_service))
        .serve(addr)
        .await?;

    Ok(())
}

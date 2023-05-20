use coin_shuffle_contracts_bindings::shared_types::Utxo;
use ethers_core::types::U256;

use self::room::Room;
use crate::rsa::{Error as RSAError, RsaPrivateKey, RsaPublicKey};
use crate::types::EncodedOutput;
use crate::{node::storage::memory::RoomStorage, rsa};

pub mod room;
pub mod storage;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("utxo doesn't exist id: {0}")]
    UtxoDoesntExist(U256),
    #[error("Storage error: {0}")]
    Storage(#[from] storage::memory::Error),
    #[error("invalid owner: {0}")]
    InvalidOwner(String),
    #[error("room with specified UTXO doesn't exist utxo_id: {0}")]
    RoomDoesntExist(U256),
    #[error("failed to decode by chunks: {0}")]
    DecodeByChunks(RSAError),
    #[error("failed to encode by chunks: {0}")]
    EncodeByChunks(RSAError),
}

#[derive(Debug, Clone)]
pub struct ShuffleRoundResult {
    pub outputs: Vec<Vec<u8>>,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Node {
    room_storage: RoomStorage,
}

impl Node {
    pub fn new(room_storage: RoomStorage) -> Self {
        Self { room_storage }
    }

    pub async fn init_room(
        &mut self,
        utxo: Utxo,
        output: Vec<u8>,
        rsa_private_key: RsaPrivateKey,
    ) -> Result<Room, Error> {
        let room = Room::new(utxo, rsa_private_key, output);

        self.room_storage.insert(&room).await?;

        Ok(room)
    }

    pub async fn update_shuffle_info(
        &mut self,
        public_keys: Vec<RsaPublicKey>,
        utxo_id: U256,
    ) -> Result<(), Error> {
        if let Some(mut room_inner) = self.room_storage.get(&utxo_id).await {
            room_inner.public_keys = public_keys;

            self.room_storage.update(&room_inner).await?;
        }

        Ok(())
    }

    pub async fn shuffle_round(
        &mut self,
        encoded_outputs: Vec<EncodedOutput>,
        utxo_id: U256,
    ) -> Result<Vec<EncodedOutput>, Error> {
        //
        // TODO: validate encoded outputs size
        //

        let mut result_outputs = Vec::<EncodedOutput>::default();

        let mut room = self
            .room_storage
            .get(&utxo_id)
            .await
            .ok_or(Error::RoomDoesntExist(utxo_id))?;

        room.participants_number = encoded_outputs.len() + room.public_keys.len() + 1;
        self.room_storage.update(&room.clone()).await?;

        for encoded_output in encoded_outputs {
            result_outputs.push(
                rsa::decode_by_chunks(encoded_output, room.clone().rsa_private_key)
                    .map_err(Error::DecodeByChunks)?,
            );
        }

        let mut nonce = Vec::<u8>::new();
        let mut encoded_self_output = room.output;
        for public_key in room.public_keys {
            let encoding_result =
                rsa::encode_by_chunks(encoded_self_output.clone(), public_key, nonce.clone())
                    .map_err(Error::EncodeByChunks)?;

            nonce = encoding_result.nonce;
            encoded_self_output = encoding_result.encoded_msg;
        }

        result_outputs.push(encoded_self_output);

        Ok(result_outputs)
    }
}

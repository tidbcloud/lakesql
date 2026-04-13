import React from 'react';
import Head from 'next/head';
import SQLQuery from '../src/SQLQuery';

const Home: React.FC = () => {
  return (
    <>
      <Head>
        <title>TiDB Cloud Lake</title>
      </Head>
      <SQLQuery />
    </>
  );
};

export default Home;